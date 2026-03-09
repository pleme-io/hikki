//! MCP server for hikki note editor.
//!
//! Provides tools for listing, searching, reading, and creating notes,
//! as well as querying backlinks in the knowledge graph.

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::Deserialize;

use crate::links::{BacklinkIndex, extract_wiki_links};
use crate::notes::Vault;

// ── Tool input types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchNotesInput {
    #[schemars(description = "Search query string. Searches note titles and content.")]
    query: String,
    #[schemars(description = "Maximum number of results to return (default: 20).")]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetNoteInput {
    #[schemars(description = "Note identifier (filename stem without .md extension).")]
    id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateNoteInput {
    #[schemars(description = "Title for the new note.")]
    title: String,
    #[schemars(description = "Optional initial content (markdown). Front matter is auto-generated.")]
    content: Option<String>,
    #[schemars(description = "Optional tags for the note.")]
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetBacklinksInput {
    #[schemars(description = "Note identifier to find backlinks for.")]
    id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConfigGetInput {
    #[schemars(description = "Config key to retrieve. Omit for full config.")]
    key: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConfigSetInput {
    #[schemars(description = "Config key to set.")]
    key: String,
    #[schemars(description = "Value to set (as JSON string).")]
    value: String,
}

// ── MCP Server ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct HikkiMcp {
    tool_router: ToolRouter<Self>,
    notes_dir: std::path::PathBuf,
}

#[tool_router]
impl HikkiMcp {
    fn new(notes_dir: std::path::PathBuf) -> Self {
        Self {
            tool_router: Self::tool_router(),
            notes_dir,
        }
    }

    fn vault(&self) -> Result<Vault, String> {
        Vault::open(&self.notes_dir).map_err(|e| format!("failed to open vault: {e}"))
    }

    // ── Standard tools ──────────────────────────────────────────────────────

    #[tool(description = "Get hikki application status and health information. Returns note count and vault path.")]
    async fn status(&self) -> String {
        let note_count = self
            .vault()
            .ok()
            .and_then(|v| v.list_notes().ok())
            .map_or(0, |n| n.len());
        serde_json::json!({
            "status": "running",
            "app": "hikki",
            "notes_dir": self.notes_dir.display().to_string(),
            "note_count": note_count,
        })
        .to_string()
    }

    #[tool(description = "Get hikki version information.")]
    async fn version(&self) -> String {
        serde_json::json!({
            "name": "hikki",
            "version": env!("CARGO_PKG_VERSION"),
            "description": env!("CARGO_PKG_DESCRIPTION"),
        })
        .to_string()
    }

    #[tool(description = "Get a hikki configuration value. Pass a key for a specific value, or omit for the full config.")]
    async fn config_get(&self, Parameters(input): Parameters<ConfigGetInput>) -> String {
        match input.key {
            Some(key) => serde_json::json!({
                "key": key,
                "value": null,
                "note": "Config queries require a running hikki instance."
            })
            .to_string(),
            None => serde_json::json!({
                "notes_dir": self.notes_dir.display().to_string(),
                "config_path": "~/.config/hikki/hikki.yaml"
            })
            .to_string(),
        }
    }

    #[tool(description = "Set a hikki configuration value at runtime.")]
    async fn config_set(&self, Parameters(input): Parameters<ConfigSetInput>) -> String {
        serde_json::json!({
            "key": input.key,
            "value": input.value,
            "applied": false,
            "note": "Config mutations require a running hikki instance."
        })
        .to_string()
    }

    // ── Note tools ──────────────────────────────────────────────────────────

    #[tool(description = "List all notes in the vault. Returns note IDs, titles, and tags.")]
    async fn list_notes(&self) -> String {
        let vault = match self.vault() {
            Ok(v) => v,
            Err(e) => return serde_json::json!({"error": e}).to_string(),
        };

        match vault.list_notes() {
            Ok(notes) => {
                let entries: Vec<serde_json::Value> = notes
                    .iter()
                    .map(|n| {
                        serde_json::json!({
                            "id": n.id,
                            "title": n.title,
                            "tags": n.tags,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "count": entries.len(),
                    "notes": entries,
                })
                .to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Search notes by query string. Searches titles and content using fuzzy matching.")]
    async fn search_notes(&self, Parameters(input): Parameters<SearchNotesInput>) -> String {
        let vault = match self.vault() {
            Ok(v) => v,
            Err(e) => return serde_json::json!({"error": e}).to_string(),
        };

        let limit = input.limit.unwrap_or(20);
        match vault.fuzzy_find(&input.query) {
            Ok(results) => {
                let entries: Vec<serde_json::Value> = results
                    .iter()
                    .take(limit)
                    .map(|n| {
                        serde_json::json!({
                            "id": n.id,
                            "title": n.title,
                            "tags": n.tags,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "query": input.query,
                    "count": entries.len(),
                    "results": entries,
                })
                .to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Get the full content of a note by its ID. Returns the note's markdown content, title, and tags.")]
    async fn get_note(&self, Parameters(input): Parameters<GetNoteInput>) -> String {
        let vault = match self.vault() {
            Ok(v) => v,
            Err(e) => return serde_json::json!({"error": e}).to_string(),
        };

        match vault.read_note(&input.id) {
            Ok(note) => serde_json::json!({
                "id": note.meta.id,
                "title": note.meta.title,
                "tags": note.meta.tags,
                "content": note.content,
            })
            .to_string(),
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Create a new note in the vault with the given title, optional content, and tags.")]
    async fn create_note(&self, Parameters(input): Parameters<CreateNoteInput>) -> String {
        let vault = match self.vault() {
            Ok(v) => v,
            Err(e) => return serde_json::json!({"error": e}).to_string(),
        };

        let id = input
            .title
            .to_lowercase()
            .replace(' ', "-")
            .replace(|c: char| !c.is_alphanumeric() && c != '-', "");

        // Build content with front matter
        let tags = input.tags.unwrap_or_default();
        let tags_str = if tags.is_empty() {
            "[]".to_string()
        } else {
            format!("[{}]", tags.join(", "))
        };

        let body = input.content.unwrap_or_default();
        let content = format!(
            "---\ntitle: {}\ntags: {tags_str}\n---\n\n# {}\n\n{body}",
            input.title, input.title
        );

        match vault.save_note(&id, &content) {
            Ok(()) => serde_json::json!({
                "id": id,
                "title": input.title,
                "created": true,
            })
            .to_string(),
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Get all notes that link to the specified note (backlinks). Useful for exploring the knowledge graph.")]
    async fn get_backlinks(&self, Parameters(input): Parameters<GetBacklinksInput>) -> String {
        let vault = match self.vault() {
            Ok(v) => v,
            Err(e) => return serde_json::json!({"error": e}).to_string(),
        };

        let title_map = match vault.build_title_map() {
            Ok(m) => m,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };

        let notes = match vault.list_notes() {
            Ok(n) => n,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };

        let mut backlink_index = BacklinkIndex::new();
        for meta in &notes {
            if let Ok(note) = vault.read_note(&meta.id) {
                let links = extract_wiki_links(&note.content);
                backlink_index.update_note(&meta.id, &links, &title_map);
            }
        }

        let backlinks = backlink_index.backlinks_for(&input.id);
        let entries: Vec<serde_json::Value> = backlinks
            .iter()
            .map(|id| serde_json::json!({"id": id}))
            .collect();

        serde_json::json!({
            "note_id": input.id,
            "count": entries.len(),
            "backlinks": entries,
        })
        .to_string()
    }
}

#[tool_handler]
impl ServerHandler for HikkiMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Hikki GPU note editor — note CRUD, full-text search, and backlink queries."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn run(notes_dir: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let server = HikkiMcp::new(notes_dir).serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
