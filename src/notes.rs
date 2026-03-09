//! Filesystem note storage — plain markdown files in a vault directory.
//!
//! Notes are `.md` files, stored flat or in subdirectories. Metadata
//! (title, tags, aliases) is extracted from YAML front matter or content.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use thiserror::Error;

/// Errors from note storage operations.
#[derive(Error, Debug)]
pub enum NoteError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("note not found: {0}")]
    NotFound(String),
}

/// Lightweight note metadata for listings.
#[derive(Debug, Clone)]
pub struct NoteMeta {
    /// Note identifier (relative path without .md extension).
    pub id: String,
    /// Display title.
    pub title: String,
    /// Last modification time.
    pub modified: SystemTime,
    /// Tags extracted from front matter or inline #tags.
    pub tags: Vec<String>,
    /// Aliases from front matter.
    pub aliases: Vec<String>,
}

/// A full note with content.
#[derive(Debug, Clone)]
pub struct Note {
    pub meta: NoteMeta,
    /// Full markdown content including front matter.
    pub content: String,
}

/// YAML front matter fields.
#[derive(Debug, Clone, Default)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
}

/// Filesystem-backed note vault.
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Open or create a vault at the given directory.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, NoteError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Get the vault root path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// List all notes in the vault (recursively).
    pub fn list_notes(&self) -> Result<Vec<NoteMeta>, NoteError> {
        let mut notes = Vec::new();
        self.scan_dir(&self.root, &mut notes)?;
        notes.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(notes)
    }

    fn scan_dir(&self, dir: &Path, notes: &mut Vec<NoteMeta>) -> Result<(), NoteError> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir(&path, notes)?;
            } else if path.extension().is_some_and(|e| e == "md") {
                if let Ok(meta) = self.read_meta(&path) {
                    notes.push(meta);
                }
            }
        }
        Ok(())
    }

    fn path_to_id(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn id_to_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.md"))
    }

    fn read_meta(&self, path: &Path) -> Result<NoteMeta, NoteError> {
        let content = fs::read_to_string(path)?;
        let metadata = fs::metadata(path)?;
        let id = self.path_to_id(path);
        let front = parse_front_matter(&content);
        let title = resolve_title(&front, &content, &id);
        let mut tags = front.tags.clone();
        // Also extract inline #tags
        extract_inline_tags(&content, &mut tags);

        Ok(NoteMeta {
            id,
            title,
            modified: metadata.modified()?,
            tags,
            aliases: front.aliases,
        })
    }

    /// Read a note by its ID.
    pub fn read_note(&self, id: &str) -> Result<Note, NoteError> {
        let path = self.id_to_path(id);
        if !path.exists() {
            return Err(NoteError::NotFound(id.to_string()));
        }
        let content = fs::read_to_string(&path)?;
        let metadata = fs::metadata(&path)?;
        let front = parse_front_matter(&content);
        let title = resolve_title(&front, &content, id);
        let mut tags = front.tags.clone();
        extract_inline_tags(&content, &mut tags);

        Ok(Note {
            meta: NoteMeta {
                id: id.to_string(),
                title,
                modified: metadata.modified()?,
                tags,
                aliases: front.aliases,
            },
            content,
        })
    }

    /// Save a note. Creates parent directories as needed.
    pub fn save_note(&self, id: &str, content: &str) -> Result<(), NoteError> {
        let path = self.id_to_path(id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        Ok(())
    }

    /// Delete a note by ID.
    pub fn delete_note(&self, id: &str) -> Result<(), NoteError> {
        let path = self.id_to_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Create a new note with optional front matter.
    pub fn create_note(&self, id: &str, title: &str) -> Result<Note, NoteError> {
        let content = format!(
            "---\ntitle: {title}\ntags: []\n---\n\n# {title}\n\n"
        );
        self.save_note(id, &content)?;

        Ok(Note {
            meta: NoteMeta {
                id: id.to_string(),
                title: title.to_string(),
                modified: SystemTime::now(),
                tags: Vec::new(),
                aliases: Vec::new(),
            },
            content,
        })
    }

    /// Build a map of note ID -> title for link resolution.
    pub fn build_title_map(&self) -> Result<HashMap<String, String>, NoteError> {
        let notes = self.list_notes()?;
        let mut map = HashMap::new();
        for note in &notes {
            // Map by ID (filename stem)
            map.insert(note.id.to_lowercase(), note.id.clone());
            // Map by title
            map.insert(note.title.to_lowercase(), note.id.clone());
            // Map by aliases
            for alias in &note.aliases {
                map.insert(alias.to_lowercase(), note.id.clone());
            }
        }
        Ok(map)
    }

    /// Simple text search across all notes (for fallback when index unavailable).
    pub fn search_text(&self, query: &str) -> Result<Vec<NoteMeta>, NoteError> {
        let query_lower = query.to_lowercase();
        let all = self.list_notes()?;
        Ok(all
            .into_iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&query_lower)
                    || n.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
                    || n.id.to_lowercase().contains(&query_lower)
            })
            .collect())
    }

    /// Fuzzy match note IDs/titles against a query string.
    pub fn fuzzy_find(&self, query: &str) -> Result<Vec<NoteMeta>, NoteError> {
        if query.is_empty() {
            return self.list_notes();
        }
        let query_lower = query.to_lowercase();
        let all = self.list_notes()?;
        let mut scored: Vec<(usize, NoteMeta)> = all
            .into_iter()
            .filter_map(|note| {
                let score = fuzzy_score(&query_lower, &note.title.to_lowercase())
                    .max(fuzzy_score(&query_lower, &note.id.to_lowercase()));
                if score > 0 {
                    Some((score, note))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(scored.into_iter().map(|(_, n)| n).collect())
    }
}

/// Simple subsequence fuzzy matching. Returns a score > 0 if query is
/// a subsequence of target, 0 otherwise. Higher = better.
fn fuzzy_score(query: &str, target: &str) -> usize {
    let query_chars: Vec<char> = query.chars().collect();
    let target_chars: Vec<char> = target.chars().collect();

    if query_chars.is_empty() {
        return 1;
    }

    let mut qi = 0;
    let mut score = 0;
    let mut prev_matched = false;

    for &tc in &target_chars {
        if qi < query_chars.len() && tc == query_chars[qi] {
            qi += 1;
            score += if prev_matched { 2 } else { 1 };
            prev_matched = true;
        } else {
            prev_matched = false;
        }
    }

    if qi == query_chars.len() { score } else { 0 }
}

/// Parse simple YAML front matter from markdown content.
pub fn parse_front_matter(content: &str) -> FrontMatter {
    let mut fm = FrontMatter::default();

    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return fm;
    }

    // Find the closing ---
    let after_open = &trimmed[3..];
    let Some(close_idx) = after_open.find("\n---") else {
        return fm;
    };

    let yaml_block = &after_open[..close_idx];
    for line in yaml_block.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix("title:") {
            fm.title = Some(title.trim().trim_matches('"').trim_matches('\'').to_string());
        } else if let Some(tags_str) = line.strip_prefix("tags:") {
            fm.tags = parse_yaml_list(tags_str);
        } else if let Some(aliases_str) = line.strip_prefix("aliases:") {
            fm.aliases = parse_yaml_list(aliases_str);
        }
    }

    fm
}

/// Parse a simple YAML inline list like `[a, b, c]` or bare values.
fn parse_yaml_list(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.starts_with('[') && s.ends_with(']') {
        s[1..s.len() - 1]
            .split(',')
            .map(|item| item.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|item| !item.is_empty())
            .collect()
    } else {
        s.split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    }
}

/// Resolve note title following the priority: front matter > heading > first line > filename.
fn resolve_title(front: &FrontMatter, content: &str, id: &str) -> String {
    if let Some(ref title) = front.title {
        if !title.is_empty() {
            return title.clone();
        }
    }

    // Strip front matter for heading search
    let body = strip_front_matter(content);

    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("# ") {
            return heading.trim().to_string();
        }
        if !trimmed.is_empty() && !trimmed.starts_with("---") {
            return trimmed.chars().take(80).collect();
        }
    }

    // Fallback to filename stem
    id.rsplit('/').next().unwrap_or(id).to_string()
}

/// Strip YAML front matter block from content, returning just the body.
pub fn strip_front_matter(content: &str) -> &str {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content;
    }
    let after_open = &trimmed[3..];
    if let Some(close_idx) = after_open.find("\n---") {
        let after_close = &after_open[close_idx + 4..];
        after_close.strip_prefix('\n').unwrap_or(after_close)
    } else {
        content
    }
}

/// Extract inline #tags from content and append to tags vec.
fn extract_inline_tags(content: &str, tags: &mut Vec<String>) {
    let body = strip_front_matter(content);
    for word in body.split_whitespace() {
        if let Some(tag) = word.strip_prefix('#') {
            let tag = tag
                .trim_end_matches(|c: char| c.is_ascii_punctuation() && c != '-' && c != '_');
            if !tag.is_empty() && !tags.contains(&tag.to_string()) {
                tags.push(tag.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_front_matter_basic() {
        let content = "---\ntitle: My Note\ntags: [rust, notes]\naliases: [my, note]\n---\n\n# My Note\n";
        let fm = parse_front_matter(content);
        assert_eq!(fm.title.as_deref(), Some("My Note"));
        assert_eq!(fm.tags, vec!["rust", "notes"]);
        assert_eq!(fm.aliases, vec!["my", "note"]);
    }

    #[test]
    fn parse_front_matter_empty() {
        let fm = parse_front_matter("no front matter here");
        assert!(fm.title.is_none());
        assert!(fm.tags.is_empty());
    }

    #[test]
    fn resolve_title_from_front_matter() {
        let fm = FrontMatter {
            title: Some("Custom Title".into()),
            ..FrontMatter::default()
        };
        assert_eq!(resolve_title(&fm, "# Heading", "file-id"), "Custom Title");
    }

    #[test]
    fn resolve_title_from_heading() {
        let fm = FrontMatter::default();
        assert_eq!(resolve_title(&fm, "# My Heading\n\nContent", "file-id"), "My Heading");
    }

    #[test]
    fn resolve_title_from_id() {
        let fm = FrontMatter::default();
        assert_eq!(resolve_title(&fm, "", "some/path/my-note"), "my-note");
    }

    #[test]
    fn strip_front_matter_works() {
        let content = "---\ntitle: test\n---\n\n# Heading\n";
        let body = strip_front_matter(content);
        assert!(body.contains("# Heading"));
        // Body starts after the closing --- plus one newline
        let trimmed = body.trim_start();
        assert!(trimmed.starts_with("# Heading"));
    }

    #[test]
    fn strip_front_matter_no_front_matter() {
        let content = "# Just a heading\n";
        assert_eq!(strip_front_matter(content), content);
    }

    #[test]
    fn extract_inline_tags_basic() {
        let mut tags = Vec::new();
        extract_inline_tags("hello #rust and #notes here", &mut tags);
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"notes".to_string()));
    }

    #[test]
    fn extract_inline_tags_dedup() {
        let mut tags = vec!["rust".to_string()];
        extract_inline_tags("hello #rust again", &mut tags);
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn fuzzy_score_exact() {
        assert!(fuzzy_score("hello", "hello") > 0);
    }

    #[test]
    fn fuzzy_score_subsequence() {
        assert!(fuzzy_score("hlo", "hello") > 0);
    }

    #[test]
    fn fuzzy_score_no_match() {
        assert_eq!(fuzzy_score("xyz", "hello"), 0);
    }

    #[test]
    fn fuzzy_score_empty_query() {
        assert!(fuzzy_score("", "anything") > 0);
    }

    #[test]
    fn vault_crud() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().join("notes")).unwrap();

        // Create
        let note = vault.create_note("test-note", "Test Note").unwrap();
        assert_eq!(note.meta.title, "Test Note");

        // Read
        let read = vault.read_note("test-note").unwrap();
        assert_eq!(read.meta.title, "Test Note");
        assert!(read.content.contains("# Test Note"));

        // List
        let all = vault.list_notes().unwrap();
        assert_eq!(all.len(), 1);

        // Save update
        vault.save_note("test-note", "# Updated\n\nNew content").unwrap();
        let updated = vault.read_note("test-note").unwrap();
        assert_eq!(updated.meta.title, "Updated");

        // Delete
        vault.delete_note("test-note").unwrap();
        let all = vault.list_notes().unwrap();
        assert!(all.is_empty());
    }

    #[test]
    fn vault_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().join("notes")).unwrap();
        assert!(vault.read_note("nonexistent").is_err());
    }

    #[test]
    fn vault_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().join("notes")).unwrap();
        vault.create_note("sub/nested-note", "Nested Note").unwrap();
        let all = vault.list_notes().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "sub/nested-note");
    }

    #[test]
    fn vault_fuzzy_find() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().join("notes")).unwrap();
        vault.create_note("meeting-notes", "Meeting Notes").unwrap();
        vault.create_note("project-ideas", "Project Ideas").unwrap();
        vault.create_note("daily-log", "Daily Log").unwrap();

        let results = vault.fuzzy_find("meet").unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "meeting-notes");
    }

    #[test]
    fn vault_search_text() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().join("notes")).unwrap();
        vault.create_note("rust-notes", "Rust Notes").unwrap();
        vault.create_note("python-notes", "Python Notes").unwrap();

        let results = vault.search_text("rust").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "rust-notes");
    }

    #[test]
    fn vault_title_map() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().join("notes")).unwrap();
        vault.create_note("my-note", "My Custom Title").unwrap();

        let map = vault.build_title_map().unwrap();
        assert!(map.contains_key("my-note"));
        assert!(map.contains_key("my custom title"));
    }
}
