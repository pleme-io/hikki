//! Hikki (筆記) — GPU-rendered markdown note editor.
//!
//! A local-first note editor with wiki links, backlink tracking,
//! full-text search, and vim-modal editing.

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use hikki::config::HikkiConfig;
use hikki::editor::EditorBuffer;
use hikki::input::{parse_command, Action, InputHandler, Mode};
use hikki::links::{extract_wiki_links, BacklinkIndex};
use hikki::notes::Vault;
use hikki::render::HikkiRenderer;
use hikki::search::SearchIndex;

#[derive(Parser)]
#[command(name = "hikki", about = "Hikki (筆記) — GPU note editor")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Search notes by query.
    Search {
        /// Search query string.
        query: String,
    },
    /// Create a new note.
    New {
        /// Note title.
        title: String,
    },
    /// List all notes.
    List,
    /// Reindex all notes for full-text search.
    Reindex,
    /// Run as MCP server (stdio transport) for Claude Code integration.
    Mcp,
}

fn load_config() -> HikkiConfig {
    match shikumi::ConfigDiscovery::new("hikki")
        .env_override("HIKKI_CONFIG")
        .discover()
    {
        Ok(path) => {
            tracing::info!("loading config from {}", path.display());
            let store =
                shikumi::ConfigStore::<HikkiConfig>::load(&path, "HIKKI_").unwrap_or_else(|e| {
                    tracing::warn!("failed to load config: {e}, using defaults");
                    let tmp = std::env::temp_dir().join("hikki-default.yaml");
                    std::fs::write(&tmp, "{}").ok();
                    shikumi::ConfigStore::load(&tmp, "HIKKI_").expect("fallback config load")
                });
            HikkiConfig::clone(&store.get())
        }
        Err(_) => {
            tracing::info!("no config file found, using defaults");
            HikkiConfig::default()
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let config = load_config();

    // Handle MCP subcommand before opening vault for GUI
    if let Some(Command::Mcp) = cli.command {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(hikki::mcp::run(config.storage.notes_dir.clone()))
            .map_err(|e| anyhow::anyhow!("MCP server error: {e}"))?;
        return Ok(());
    }

    let vault = Vault::open(&config.storage.notes_dir)?;

    match cli.command {
        Some(Command::Mcp) => unreachable!("handled above"),
        Some(Command::Search { query }) => {
            cmd_search(&vault, &config, &query)?;
        }
        Some(Command::New { title }) => {
            cmd_new(&vault, &title)?;
        }
        Some(Command::List) => {
            cmd_list(&vault)?;
        }
        Some(Command::Reindex) => {
            cmd_reindex(&vault)?;
        }
        None => {
            launch_gui(&config, &vault)?;
        }
    }

    Ok(())
}

fn cmd_search(vault: &Vault, config: &HikkiConfig, query: &str) -> Result<()> {
    tracing::info!("searching notes for: {query}");

    // Try full-text search first
    let index_dir = SearchIndex::default_index_dir();
    if let Ok(index) = SearchIndex::open(&index_dir) {
        let results = index.search(query, config.search.max_results as usize)?;
        if !results.is_empty() {
            for result in &results {
                println!("{}: {} (score: {:.2})", result.id, result.title, result.score);
            }
            println!("\n{} result(s) found.", results.len());
            return Ok(());
        }
    }

    // Fall back to text search
    let results = vault.search_text(query)?;
    if results.is_empty() {
        println!("No notes found matching: {query}");
    } else {
        for note in &results {
            println!("{}: {}", note.id, note.title);
        }
        println!("\n{} note(s) found.", results.len());
    }
    Ok(())
}

fn cmd_new(vault: &Vault, title: &str) -> Result<()> {
    let id = title
        .to_lowercase()
        .replace(' ', "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "");
    let note = vault.create_note(&id, title)?;
    println!("Created: {} ({})", note.meta.title, note.meta.id);
    Ok(())
}

fn cmd_list(vault: &Vault) -> Result<()> {
    let notes = vault.list_notes()?;
    if notes.is_empty() {
        println!("No notes found in vault.");
    } else {
        for note in &notes {
            let tags = if note.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", note.tags.join(", "))
            };
            println!("{}: {}{tags}", note.id, note.title);
        }
        println!("\n{} note(s) total.", notes.len());
    }
    Ok(())
}

fn cmd_reindex(vault: &Vault) -> Result<()> {
    let index_dir = SearchIndex::default_index_dir();
    let index = SearchIndex::open(&index_dir)?;
    let count = index.reindex_vault(vault)?;
    println!("Reindexed {count} notes.");
    Ok(())
}

fn launch_gui(config: &HikkiConfig, vault: &Vault) -> Result<()> {
    tracing::info!("launching hikki GUI");
    tracing::info!("notes dir: {}", config.storage.notes_dir.display());

    let font_size = config.appearance.font_size;
    let line_height = font_size * config.appearance.line_spacing;
    let mut renderer = HikkiRenderer::new(font_size, line_height);

    // Load initial note list
    if let Ok(notes) = vault.list_notes() {
        renderer.state.note_list = notes.iter().map(|n| n.title.clone()).collect();
        // Open first note if available
        if let Some(first) = notes.first() {
            if let Ok(note) = vault.read_note(&first.id) {
                renderer.state.buffer = EditorBuffer::from_text(&note.content);
                renderer.state.buffer.set_file_path(&first.id);
            }
        }
    }

    renderer.state.show_preview = config.preview.enabled;
    renderer.state.show_note_list = true;

    // Build the search index in the background
    let index_dir = SearchIndex::default_index_dir();
    let search_index = SearchIndex::open(&index_dir).ok();
    if let Some(ref idx) = search_index {
        if let Err(e) = idx.reindex_vault(vault) {
            tracing::warn!("initial reindex failed: {e}");
        }
    }

    // Build backlink index
    let mut backlink_index = BacklinkIndex::new();
    if let Ok(title_map) = vault.build_title_map() {
        if let Ok(notes) = vault.list_notes() {
            for meta in &notes {
                if let Ok(note) = vault.read_note(&meta.id) {
                    let links = extract_wiki_links(&note.content);
                    backlink_index.update_note(&meta.id, &links, &title_map);
                }
            }
        }
    }

    // Clipboard for yank/paste
    let mut clipboard = String::new();
    let mut input_handler = InputHandler::new();

    // Capture vault root for use in the closure
    let notes_dir = config.storage.notes_dir.clone();
    let _max_results = config.search.max_results as usize;

    madori::App::builder(renderer)
        .title("Hikki (筆記)")
        .size(config.appearance.width, config.appearance.height)
        .on_event(move |event, renderer: &mut HikkiRenderer| {
            use madori::event::{AppEvent, KeyEvent};
            use madori::EventResponse;

            let AppEvent::Key(KeyEvent {
                key,
                pressed: true,
                modifiers,
                text: _,
            }) = event
            else {
                return EventResponse::default();
            };

            let action = input_handler.handle_key(*key, *modifiers);

            match action {
                // -- Cursor movement --
                Action::MoveLeft => renderer.state.buffer.move_left(),
                Action::MoveRight => renderer.state.buffer.move_right(),
                Action::MoveUp => renderer.state.buffer.move_up(),
                Action::MoveDown => renderer.state.buffer.move_down(),
                Action::MoveWordForward => renderer.state.buffer.move_word_forward(),
                Action::MoveWordBackward => renderer.state.buffer.move_word_backward(),
                Action::MoveLineStart => renderer.state.buffer.move_to_line_start(),
                Action::MoveLineEnd => renderer.state.buffer.move_to_line_end(),
                Action::MoveDocStart => renderer.state.buffer.move_to_doc_start(),
                Action::MoveDocEnd => renderer.state.buffer.move_to_doc_end(),
                Action::MoveHalfPageDown => {
                    let visible = renderer.visible_lines();
                    renderer.state.buffer.move_half_page_down(visible);
                }
                Action::MoveHalfPageUp => {
                    let visible = renderer.visible_lines();
                    renderer.state.buffer.move_half_page_up(visible);
                }

                // -- Editing --
                Action::InsertChar(c) => {
                    // Handle insert mode text from the event
                    renderer.state.buffer.insert_char(c);
                }
                Action::InsertNewline => renderer.state.buffer.insert_char('\n'),
                Action::DeleteBack => renderer.state.buffer.delete_back(),
                Action::DeleteForward => renderer.state.buffer.delete_forward(),
                Action::DeleteLine => renderer.state.buffer.delete_line(),
                Action::YankLine => {
                    clipboard = renderer.state.buffer.yank_line();
                }
                Action::PasteBelow => {
                    if !clipboard.is_empty() {
                        renderer.state.buffer.paste_below(&clipboard);
                    }
                }
                Action::OpenLineBelow => renderer.state.buffer.open_line_below(),
                Action::OpenLineAbove => renderer.state.buffer.open_line_above(),

                // -- Mode changes --
                Action::EnterInsertMode => {
                    renderer.state.mode = Mode::Insert;
                }
                Action::EnterInsertModeAppend => {
                    renderer.state.buffer.move_right();
                    renderer.state.mode = Mode::Insert;
                }
                Action::EnterVisualMode => {
                    renderer.state.buffer.start_selection();
                    renderer.state.mode = Mode::Visual;
                }
                Action::EnterCommandMode => {
                    renderer.state.mode = Mode::Command;
                    renderer.state.command_text.clear();
                }
                Action::EnterSearchMode => {
                    renderer.state.mode = Mode::Search;
                    renderer.state.search_query.clear();
                }
                Action::ExitToNormal => {
                    renderer.state.buffer.clear_selection();
                    renderer.state.mode = Mode::Normal;
                    renderer.state.command_text.clear();
                }

                // -- Selection --
                Action::ExtendSelection => {
                    renderer.state.buffer.extend_selection_to_cursor();
                }
                Action::YankSelection => {
                    if let Some(text) = renderer.state.buffer.selected_text() {
                        clipboard = text;
                    }
                    renderer.state.buffer.clear_selection();
                }
                Action::DeleteSelection => {
                    renderer.state.buffer.clear_selection();
                }

                // -- Undo/Redo --
                Action::Undo => renderer.state.buffer.undo(),
                Action::Redo => renderer.state.buffer.redo(),

                // -- Leader sequences --
                Action::TogglePreview => {
                    renderer.state.show_preview = !renderer.state.show_preview;
                }
                Action::ToggleNoteList => {
                    renderer.state.show_note_list = !renderer.state.show_note_list;
                }
                Action::NewNote => {
                    // Create a new untitled buffer
                    renderer.state.buffer = EditorBuffer::from_text(
                        "---\ntitle: Untitled\ntags: []\n---\n\n# Untitled\n\n",
                    );
                    renderer.state.buffer.set_cursor(6, 0);
                    renderer.state.mode = Mode::Insert;
                }
                Action::FindFile => {
                    renderer.state.mode = Mode::Search;
                    renderer.state.search_query.clear();
                }
                Action::SearchVault => {
                    renderer.state.mode = Mode::Search;
                    renderer.state.search_query.clear();
                }
                Action::FollowLink => {
                    // Try to follow a wiki link at cursor
                    let content = renderer.state.buffer.text();
                    let cursor = renderer.state.buffer.cursor();
                    let links = hikki::links::extract_wiki_links(&content);
                    // Find a link whose range covers the cursor line
                    for link in &links {
                        // Simple heuristic: check if any link target matches on this line
                        let line_text = renderer.state.buffer.line_text(cursor.line);
                        if line_text.contains(&format!("[[{}]]", link.target))
                            || line_text.contains(&format!("[[{}|", link.target))
                        {
                            if let Ok(v) = Vault::open(&notes_dir) {
                                let target_lower = link.target.to_lowercase();
                                if let Ok(title_map) = v.build_title_map() {
                                    if let Some(id) = title_map.get(&target_lower) {
                                        if let Ok(note) = v.read_note(id) {
                                            renderer.state.buffer =
                                                EditorBuffer::from_text(&note.content);
                                            renderer.state.buffer.set_file_path(id);
                                            renderer.state.scroll_offset = 0;
                                            // Update backlinks display
                                            let _bl = hikki::links::extract_wiki_links(&note.content);
                                            break;
                                        }
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
                Action::ShowBacklinks => {
                    // Toggle backlinks display is already handled in rendering
                }

                // -- Note list navigation --
                Action::NoteListNext => {
                    if renderer.state.note_list_selected + 1 < renderer.state.note_list.len() {
                        renderer.state.note_list_selected += 1;
                    }
                }
                Action::NoteListPrev => {
                    renderer.state.note_list_selected =
                        renderer.state.note_list_selected.saturating_sub(1);
                }
                Action::NoteListOpen => {
                    // Open selected note
                }

                // -- Command/Search mode --
                Action::CommandInput(c) => {
                    renderer.state.command_text.push(c);
                }
                Action::CommandSubmit => {
                    let cmd = renderer.state.command_text.clone();
                    let cmd_action = parse_command(&cmd);
                    match cmd_action {
                        Action::Quit => {
                            return EventResponse {
                                exit: true,
                                ..Default::default()
                            };
                        }
                        Action::Save => {
                            if let Some(id) = renderer.state.buffer.file_path().map(str::to_string) {
                                let content = renderer.state.buffer.text();
                                if let Ok(v) = Vault::open(&notes_dir) {
                                    if v.save_note(&id, &content).is_ok() {
                                        renderer.state.buffer.mark_saved();
                                        renderer.state.status_message = "Saved.".into();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    renderer.state.command_text.clear();
                    renderer.state.mode = Mode::Normal;
                }
                Action::CommandCancel => {
                    renderer.state.command_text.clear();
                    renderer.state.mode = Mode::Normal;
                }
                Action::SearchInput(c) => {
                    renderer.state.search_query.push(c);
                }
                Action::SearchSubmit => {
                    // Execute search and open first result
                    let query = renderer.state.search_query.clone();
                    if !query.is_empty() {
                        if let Ok(v) = Vault::open(&notes_dir) {
                            let results = v.fuzzy_find(&query).unwrap_or_default();
                            if let Some(first) = results.first() {
                                if let Ok(note) = v.read_note(&first.id) {
                                    renderer.state.buffer =
                                        EditorBuffer::from_text(&note.content);
                                    renderer.state.buffer.set_file_path(&first.id);
                                    renderer.state.scroll_offset = 0;
                                }
                            }
                        }
                    }
                    renderer.state.mode = Mode::Normal;
                }
                Action::SearchCancel => {
                    renderer.state.search_query.clear();
                    renderer.state.mode = Mode::Normal;
                }

                // -- File operations --
                Action::Save => {
                    if let Some(id) = renderer.state.buffer.file_path().map(str::to_string) {
                        let content = renderer.state.buffer.text();
                        if let Ok(v) = Vault::open(&notes_dir) {
                            if v.save_note(&id, &content).is_ok() {
                                renderer.state.buffer.mark_saved();
                                renderer.state.status_message = "Saved.".into();
                                // Re-index if configured
                                if let Ok(idx) = SearchIndex::open(SearchIndex::default_index_dir()) {
                                    if let Ok(note) = v.read_note(&id) {
                                        idx.index_note(&note).ok();
                                    }
                                }
                            }
                        }
                    }
                }

                Action::Quit => {
                    return EventResponse {
                        exit: true,
                        ..Default::default()
                    };
                }

                Action::NoOp => {}
            }

            // Update mode in view state
            renderer.state.mode = input_handler.mode();

            // Keep cursor visible
            renderer.ensure_cursor_visible();

            EventResponse::consumed()
        })
        .run()
        .map_err(|e| anyhow::anyhow!("app error: {e}"))?;

    Ok(())
}
