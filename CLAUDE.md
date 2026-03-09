# Hikki (筆記) -- GPU Note Editor

Binary: `hikki` | Crate: `hikki` | Config: `~/.config/hikki/hikki.yaml`

## Build & Test

```bash
cargo build                          # compile
cargo test --lib                     # unit tests
cargo test                           # all tests
cargo run                            # launch GUI editor
cargo run -- search "query"          # search notes from CLI
cargo run -- new "Title"             # create new note from CLI
cargo run -- daily                   # open/create today's daily note
cargo run -- graph                   # launch graph view
nix build                            # Nix build
nix run .#regenerate                 # regenerate Cargo.nix after dep changes
```

## Current State

Minimal scaffold with filesystem-backed note storage and CLI search:

- `config.rs` -- `HikkiConfig` with appearance/editor/storage/search/sync sections.
  Working shikumi integration. 2 tests. Config struct is solid but needs expansion
  for graph, keybindings, and plugin settings.
- `platform/mod.rs` -- `NoteStorage` trait with `NoteMeta`/`Note` types. Clean
  trait boundary for list/read/save/delete/search operations.
- `platform/macos/mod.rs` -- `MacOSNoteStorage`: filesystem-backed markdown storage.
  Working list, read, save, delete, and text search. Title extraction from headings,
  tag extraction from `tags:` lines.
- `main.rs` -- CLI with Search subcommand. GUI launch is a stub.
- `lib.rs` -- Re-exports config + platform.

**Missing entirely:** GPU rendering, text editing, knowledge graph, wiki links,
backlinks, full-text search index (sakuin is a dependency but not used), markdown
preview, daily notes, templates, MCP server, scripting. The core note storage
abstraction is decent but everything above it needs building.

**NOTE:** `NoteStorage` trait is not platform-specific -- the `MacOSNoteStorage`
implementation is pure filesystem operations with no macOS APIs. Consider renaming
to `FilesystemNoteStorage` and removing the platform abstraction until actual
platform-specific behavior is needed (e.g., iCloud integration).

## Competitive Landscape

| Competitor | Stack | Strengths | Weaknesses vs hikki |
|-----------|-------|-----------|---------------------|
| Obsidian | Electron | Wiki links, graph view, 1800+ plugins, canvas, themes | Electron bloat, proprietary, JS plugins not Rhai, no GPU |
| Logseq | ClojureScript | Outliner, bidirectional links, open-source, journals | ClojureScript heavy, outliner-only, no GPU, no vim-modal |
| Zettlr | Electron | Academic writing, Zettelkasten, citations, pandoc | Electron bloat, no knowledge graph visualization, no vim |
| Joplin | Electron | E2E encrypted sync, web clipper, Markdown | Electron bloat, no graph view, no vim-modal, no scripting |
| Notion | Electron | Blocks, databases, collaboration, templates | Electron bloat, cloud-only, proprietary, no local files |
| Neovim + plugins | Lua | telekasten.nvim, obsidian.nvim, ultimate flexibility | Terminal-only, no graph visualization, fragmented plugin ecosystem |

**Key differentiators:**
- GPU-rendered via garasu (not Electron), native performance
- Knowledge graph visualization with force-directed GPU layout
- Vim-modal editing with full Normal/Insert/Visual/Command modes
- MCP server for AI-assisted note workflows (summarize, link, tag)
- Rhai scripting (not JavaScript/Lua plugins)
- Local-first: plain markdown files, your filesystem, your data
- Nix-configured, declarative, hot-reloadable

## Architecture

### Data Flow

```
  ~/.config/hikki/hikki.yaml ──> shikumi ConfigStore
                                       |
  Vault Directory (plain .md files)    |
       |                               v
       v                         HikkiConfig
  File Scanner ──> Front Matter Parser
       |                |
       v                v
  sakuin/tantivy    Wiki Link Extractor ──> Backlink Index
  Full-text Index        |                       |
       |                 v                       v
       v           Graph Builder ──> Force-directed Layout
  Search Results         |                       |
       |                 v                       v
       v           Graph Renderer ──────> GPU Render (garasu)
  Editor Buffer ──> Markdown Parser (mojiban)    |
       |                 |                       v
       v                 v                 winit Window
  GPU Text Render   Live Preview Panel    (Metal/Vulkan)
```

### Module Map

| Module | Responsibility | Key Types | pleme-io Deps |
|--------|---------------|-----------|---------------|
| `editor` | Text editing buffer, cursor, selection, undo/redo | `EditorBuffer`, `Cursor`, `Selection`, `UndoStack` | garasu, mojiban |
| `graph` | Knowledge graph: link extraction, backlinks, layout | `Graph`, `Node`, `Edge`, `ForceLayout` | garasu |
| `storage` | Vault filesystem operations, front matter, file watching | `Vault`, `NoteFile`, `FrontMatter` | -- |
| `search` | Full-text index, tag index, fuzzy filename matching | `SearchIndex`, `SearchResult` | sakuin (tantivy) |
| `render` | GPU rendering: editor, preview, graph, panels | `Renderer`, `EditorView`, `PreviewView`, `GraphView` | garasu, madori, egaku |
| `markdown` | Markdown parsing, wiki link detection, completion | `MarkdownDoc`, `WikiLink`, `Completion` | mojiban |
| `daily` | Daily note creation, date-based templates | `DailyNote`, `DateTemplate` | -- |
| `template` | Note templates, variable substitution | `Template`, `TemplateRegistry` | -- |
| `sync` | Sync backends: git, WebDAV (plugin-provided) | `SyncBackend` trait | -- |
| `config` | Config struct, shikumi integration | `HikkiConfig` | shikumi |
| `platform` | Note storage trait (to be renamed/simplified) | `NoteStorage`, `NoteMeta`, `Note` | -- |
| `mcp` | (planned) MCP server for automation | -- | kaname |
| `plugin` | (planned) Rhai scripting engine | -- | soushi |

### Planned Source Layout

```
src/
  main.rs              # CLI entry point (clap)
  config.rs            # HikkiConfig + shikumi
  lib.rs               # Library root
  editor/
    mod.rs             # EditorBuffer: text manipulation, cursor, selection
    cursor.rs          # Cursor position, multi-cursor support
    selection.rs       # Selection ranges, visual mode
    undo.rs            # Undo/redo stack with operation coalescing
    completion.rs      # Wiki link [[, tag #, command : completion
  graph/
    mod.rs             # Knowledge graph: nodes, edges, queries
    links.rs           # Wiki link extraction, resolution, backlink index
    layout.rs          # Force-directed layout algorithm (GPU-accelerated)
    render.rs          # Graph node/edge rendering via garasu
  storage/
    mod.rs             # Vault: file operations, directory structure
    frontmatter.rs     # YAML front matter parsing (title, tags, aliases)
    watcher.rs         # FSEvents/inotify file watcher for live reload
  search/
    mod.rs             # Search orchestrator
    fulltext.rs        # tantivy/sakuin full-text index
    tags.rs            # Tag index and filtering
    fuzzy.rs           # Fuzzy filename matching
  render/
    mod.rs             # Render orchestration (madori RenderCallback)
    editor_view.rs     # Editor panel rendering
    preview_view.rs    # Live markdown preview rendering
    graph_view.rs      # Knowledge graph rendering
    sidebar.rs         # File tree, backlinks panel, tag cloud
    status_bar.rs      # Mode indicator, word count, file path
  markdown/
    mod.rs             # Markdown parsing and wiki link detection
    wiki_link.rs       # [[link]] and [[link|alias]] parsing
    completion.rs      # Inline completion provider
  daily.rs             # Daily note management
  template.rs          # Note templates
  sync.rs              # Sync backend trait and implementations
  mcp.rs               # MCP server (kaname)
  plugin.rs            # Rhai scripting (soushi)
```

## pleme-io Library Integration

| Library | Role in hikki |
|---------|--------------|
| **shikumi** | Config discovery + hot-reload for `HikkiConfig` |
| **garasu** | GPU context, text rendering for editor and preview |
| **madori** | App framework: event loop, render loop, input dispatch |
| **egaku** | Widgets: file tree, tab bar, split panes, modals, search dialog |
| **irodzuki** | Base16 theme to GPU uniforms for consistent theming |
| **mojiban** | Markdown to styled spans for live preview, syntax highlighting for code blocks |
| **kaname** | MCP server scaffold for note automation tools |
| **soushi** | Rhai scripting for user plugins and templates |
| **awase** | Hotkey registration, vim-modal key parsing |
| **hasami** | Clipboard for copy/paste operations |
| **tsuuchi** | Notifications for sync complete, auto-save, reminders |

Libraries NOT used:
- **oto** -- no audio
- **tsunagu** -- sync daemon uses simpler approach (periodic timer, not full IPC)
- **todoku** -- no HTTP API calls (sync is git/filesystem-based)

## Implementation Phases

### Phase 1: Editor Buffer
Build the core text editing engine:
1. `EditorBuffer`: gap buffer or rope (ropey crate) for efficient text manipulation
2. `Cursor`: line/column position, movement by char/word/line/paragraph
3. `Selection`: visual selection ranges, multi-cursor support
4. `UndoStack`: undo/redo with operation coalescing (group rapid keystrokes)
5. Basic text operations: insert, delete, backspace, newline, indent/outdent
6. Line operations: duplicate, move up/down, delete line, join lines

### Phase 2: GPU Editor Rendering
Render the editor via madori + garasu:
1. madori app shell with `RenderCallback`
2. Editor panel: monospace text rendering via garasu `TextRenderer`
3. Line numbers, cursor rendering, selection highlight
4. Syntax highlighting for markdown via mojiban
5. Scrolling: smooth scroll, scroll-past-end, center cursor option
6. Split panes via egaku `SplitPane`: editor | preview (horizontal/vertical)

### Phase 3: Markdown Preview
Live side-by-side markdown preview:
1. Parse markdown via mojiban `MarkdownParser`
2. Render styled spans via garasu text rendering
3. Headings, bold, italic, code blocks, lists, blockquotes, links
4. Inline code with syntax highlighting via mojiban `SyntaxHighlighter`
5. Synchronized scrolling between editor and preview
6. Toggle preview: side-by-side, preview-only, editor-only

### Phase 4: Wiki Links and Backlinks
Obsidian-compatible wiki link system:
1. Parse `[[link]]` and `[[link|display text]]` syntax
2. Link resolution: match against note titles and filenames
3. Backlink index: for each note, track which notes link to it
4. Backlinks panel: sidebar showing all notes that reference current note
5. `[[` triggers completion popup with fuzzy-matched note titles
6. `gd` (go to definition) follows wiki link under cursor
7. Broken link detection: highlight links to non-existent notes

### Phase 5: Knowledge Graph
GPU-rendered knowledge graph visualization:
1. Build graph from wiki link relationships
2. Force-directed layout algorithm (Barnes-Hut for performance)
3. GPU rendering: nodes as circles, edges as lines, labels as garasu text
4. Interactive: click node to open note, drag to rearrange
5. Zoom/pan navigation (`hjkl` pan, `+`/`-` zoom)
6. Filters: depth (1-hop, 2-hop, all), tag filter, orphan highlighting
7. Focus mode: center graph on current note, fade distant nodes
8. Color coding: by tag, by creation date, by link count

### Phase 6: Full-text Search
Replace naive string search with proper index:
1. sakuin (tantivy wrapper) for full-text indexing
2. Index on save (configurable, default: on)
3. Background re-indexing on vault changes (file watcher)
4. Search UI: search dialog with real-time results
5. Result ranking: title match > heading match > body match
6. Tag search: `#tag` queries against tag index
7. Link search: `[[link]]` queries against link index
8. Fuzzy filename: partial matches for quick note opening

### Phase 7: Daily Notes and Templates
Structured note creation:
1. Daily note: `hikki daily` creates/opens `daily/YYYY-MM-DD.md`
2. Daily template: configurable template with date variables
3. Weekly/monthly rollups: aggregate daily notes
4. Templates: `~/.config/hikki/templates/*.md` with variable substitution
5. Variables: `{{date}}`, `{{time}}`, `{{title}}`, `{{cursor}}`
6. Template picker: command palette integration

### Phase 8: Front Matter
YAML front matter support:
1. Parse `---` delimited YAML front matter blocks
2. Standard fields: `title`, `tags`, `aliases`, `created`, `modified`
3. Custom fields: arbitrary YAML preserved on save
4. Front matter editing: structured form or raw YAML
5. Tags from front matter integrated into search and graph
6. Aliases for wiki link resolution (note can be reached by multiple names)

### Phase 9: Sync Backends
Optional sync via configurable backends:
1. Git sync: commit on save, periodic push/pull
2. WebDAV sync (plugin-provided)
3. Conflict resolution: last-write-wins with `.conflict` file preservation
4. Background sync via launchd/systemd service (already wired in HM module)

### Phase 10: MCP Server
Embedded MCP server via kaname (stdio transport):
1. Standard tools: `status`, `config_get`, `config_set`, `version`
2. Note CRUD: `create_note`, `read_note`, `update_note`, `delete_note`
3. Search tools: `search`, `search_tags`, `search_links`
4. Graph tools: `get_backlinks`, `get_graph`, `get_orphans`
5. Daily tools: `get_daily`, `create_daily`
6. Template tools: `list_templates`, `insert_template`
7. Export tools: `export_note` (md, html, pdf via pandoc)
8. Vault tools: `vault_stats` (note count, tag count, link count, orphan count)

### Phase 11: Plugin System
Rhai scripting via soushi:
1. Script loading from `~/.config/hikki/scripts/*.rhai`
2. Rhai API: `hikki.note_new(title)`, `hikki.note_open(path)`,
   `hikki.search(query)`, `hikki.tags()`, `hikki.backlinks(path)`,
   `hikki.graph_focus(path)`, `hikki.daily()`, `hikki.template(name)`,
   `hikki.export(format, path)`, `hikki.vault_stats()`,
   `hikki.insert(text)`, `hikki.cursor()`, `hikki.selection()`
3. Event hooks: `on_note_open`, `on_note_save`, `on_note_create`,
   `on_daily_create`, `on_link_follow`
4. Custom commands for command palette
5. Plugin manifest: `plugin.toml`

## Hotkey System

Modal keybindings via awase, heavily inspired by vim:

**Normal mode (default):**
| Key | Action |
|-----|--------|
| `j` / `k` | Cursor down / up |
| `h` / `l` | Cursor left / right |
| `w` / `b` | Word forward / backward |
| `0` / `$` | Line start / end |
| `gg` / `G` | Document top / bottom |
| `Ctrl+d` / `Ctrl+u` | Half-page down / up |
| `/` | Search in current note |
| `gd` | Go to definition (follow wiki link) |
| `gb` | Go to backlinks panel |
| `gf` | Go to file (fuzzy finder) |
| `gt` / `gT` | Next / previous tab |
| `i` | Enter insert mode |
| `v` | Enter visual mode |
| `o` / `O` | New line below / above (enter insert) |
| `dd` | Delete line |
| `yy` | Yank line |
| `p` / `P` | Paste below / above |
| `u` / `Ctrl+r` | Undo / redo |
| `:` | Enter command mode |
| `Space` | Leader key prefix |

**Leader key (`Space`) sequences:**
| Key | Action |
|-----|--------|
| `Space f` | Find file (fuzzy) |
| `Space s` | Search vault (full-text) |
| `Space g` | Open graph view |
| `Space d` | Open/create daily note |
| `Space t` | Tag search |
| `Space n` | New note |
| `Space p` | Toggle preview |
| `Space b` | Toggle backlinks panel |
| `Space e` | Toggle file explorer |

**Insert mode:**
| Key | Action |
|-----|--------|
| (all keys) | Insert text |
| `[[` | Trigger wiki link completion |
| `#` | Trigger tag completion |
| `Esc` | Return to normal mode |
| `Ctrl+s` | Save note |

**Visual mode:**
| Key | Action |
|-----|--------|
| `h` / `j` / `k` / `l` | Extend selection |
| `y` | Yank (copy) selection |
| `d` | Delete selection |
| `>` / `<` | Indent / outdent selection |
| `Esc` | Return to normal mode |

**Command mode:**
`:new [title]`, `:open <path>`, `:save`, `:search <query>`, `:graph`,
`:daily`, `:template <name>`, `:export <format> [path]`, `:tags`,
`:backlinks`, `:split h|v`, `:quit`, `:wq`

**Graph mode (when graph view focused):**
| Key | Action |
|-----|--------|
| `h` / `j` / `k` / `l` | Pan graph |
| `+` / `-` | Zoom in / out |
| `Enter` | Open focused node as note |
| `f` | Focus on current note |
| `d` | Cycle depth filter (1/2/3/all) |
| `t` | Toggle tag coloring |
| `Esc` | Exit graph view |

## Configuration

### Config Struct

```yaml
# ~/.config/hikki/hikki.yaml
appearance:
  width: 800
  height: 600
  font_size: 15.0
  opacity: 0.95
  line_spacing: 1.5
  line_numbers: true
  cursor_blink: true
  cursor_style: block          # block, line, underline
editor:
  tab_size: 4
  word_wrap: true
  spell_check: true
  auto_save_secs: 30           # 0 to disable
  auto_pairs: true             # auto-close brackets, quotes
  trailing_whitespace: trim    # trim, highlight, ignore
  format_on_save: false
storage:
  notes_dir: ~/Documents/hikki
  format: markdown
  auto_backup: true
  daily_dir: daily             # relative to notes_dir
  template_dir: null           # null = ~/.config/hikki/templates
  front_matter: true           # auto-generate front matter on new notes
search:
  index_on_save: true
  max_results: 50
  fuzzy_threshold: 0.3         # 0.0 = exact, 1.0 = very fuzzy
graph:
  layout: force_directed       # force_directed, radial, tree
  node_size: 8.0
  edge_opacity: 0.3
  label_size: 12.0
  max_depth: 3                 # default depth filter
  color_by: tags               # tags, date, links, none
  orphan_highlight: true
sync:
  enable: false
  method: git                  # git, webdav
  remote_url: null
  auto_commit: true
  commit_message: "hikki: auto-save"
  push_interval_secs: 300      # 0 = push on every commit
preview:
  enabled: true
  position: right              # right, bottom, float
  width_ratio: 0.5             # fraction of window for preview
  sync_scroll: true
keybindings: {}                # override default keybindings
```

### Env Overrides

- `HIKKI_CONFIG=/path/to/config.yaml` -- full config path override
- `HIKKI_STORAGE__NOTES_DIR=~/notes` -- individual field override
- `HIKKI_EDITOR__TAB_SIZE=2` -- nested field (double underscore)
- `HIKKI_SYNC__ENABLE=true` -- nested field

## Note Format

### File Structure

Notes are plain markdown files (`.md`) in the vault directory. Subdirectories
are supported for organization. The vault is just a directory of files -- no
database, no proprietary format.

```
~/Documents/hikki/
  daily/
    2026-03-09.md
    2026-03-08.md
  projects/
    hikki-roadmap.md
  ideas/
    gpu-note-editor.md
  meeting-notes.md
  reading-list.md
```

### Front Matter

Optional YAML front matter at the top of each file:

```markdown
---
title: GPU Note Editor Ideas
tags: [project, hikki, gpu]
aliases: [note editor, hikki ideas]
created: 2026-03-09T10:00:00
---

# GPU Note Editor Ideas

Content here...
```

### Wiki Links

Obsidian-compatible syntax:
- `[[note-name]]` -- link to note by filename (without .md extension)
- `[[note-name|display text]]` -- link with custom display text
- `[[note-name#heading]]` -- link to specific heading
- `#tag` -- inline tag (added to note's tag index)

### Title Resolution Order

1. Front matter `title` field
2. First `# heading` in content
3. First non-empty line
4. Filename stem

## Nix Integration

### Flake Structure

Uses substrate `rust-tool-release-flake.nix` for multi-platform packages.

**Exports:**
- `packages.{system}.{hikki,default}` -- the binary
- `overlays.default` -- `pkgs.hikki`
- `homeManagerModules.default` -- HM module at `blackmatter.components.hikki`
- `devShells.{system}.default` -- dev environment
- `apps.{system}.{check-all,bump,publish,release,regenerate}` -- substrate apps

### HM Module (`module/default.nix`)

Already exists with typed options matching the current config struct:

- `blackmatter.components.hikki.enable`
- `blackmatter.components.hikki.package`
- `blackmatter.components.hikki.appearance.{width, height, font_size, opacity, line_spacing}`
- `blackmatter.components.hikki.editor.{tab_size, word_wrap, spell_check, auto_save_secs}`
- `blackmatter.components.hikki.storage.{notes_dir, format, auto_backup}`
- `blackmatter.components.hikki.search.{index_on_save, max_results}`
- `blackmatter.components.hikki.sync.{enable, method, remote_url}`
- `blackmatter.components.hikki.extraSettings`

Generates YAML via `lib.generators.toYAML` to `xdg.configFile."hikki/hikki.yaml"`.
Includes launchd/systemd service for sync daemon (when `sync.enable = true`).

Update the HM module when new config sections are added (graph, preview, keybindings).

## Design Decisions

### Text Buffer
- **Rope (ropey crate)** over gap buffer: O(log n) insert/delete at any position.
  Handles large files well. Memory-efficient for long notes. Used by xi-editor,
  helix, and other Rust editors.
- **Operation-based undo**: Store operations (insert, delete) not snapshots.
  Coalesce rapid keystrokes into single undo entries (group by 500ms pauses).

### Note Storage
- **Plain markdown files**: No database, no proprietary format. Notes are just
  `.md` files in a directory. Compatible with Obsidian, Logseq, any markdown tool.
- **Front matter for metadata**: Optional YAML front matter for title, tags, aliases.
  Notes without front matter work fine (title from heading, no tags).
- **No database for notes**: The filesystem IS the database. sakuin/tantivy indexes
  the files for fast search but is regenerated from source files on rebuild.

### Wiki Links
- **Obsidian-compatible syntax**: `[[note]]` and `[[note|alias]]`. Maximize
  interoperability with existing note ecosystems.
- **Resolution**: Match by filename stem (case-insensitive), then by front matter
  `aliases` field. Ambiguous links show disambiguation popup.

### Knowledge Graph
- **Force-directed layout**: Barnes-Hut O(n log n) approximation for large graphs.
  GPU-accelerated position updates via compute shader (future optimization).
- **Incremental updates**: Don't rebuild entire graph on every edit. Track link
  additions/removals and update affected nodes only.

### Search Architecture
- **sakuin (tantivy)**: Full-text search with ranking. Already a Cargo dependency.
  Index stored in `~/.cache/hikki/index/`. Rebuilt on startup if stale.
- **Layered search**: Quick filename fuzzy match (no index needed) for `:open`,
  full-text tantivy search for vault-wide queries.

### Platform Considerations
- **`NoteStorage` trait refactoring**: Current `MacOSNoteStorage` is pure filesystem
  operations with no macOS-specific APIs. Rename to `FilesystemNoteStorage` and
  make it the default implementation. Add platform-specific implementations only
  when needed (e.g., iCloud integration via macOS APIs).
- **Cross-platform**: Editor, graph, search, rendering are all cross-platform.
  Only sync backends may have platform-specific implementations.

## Testing Strategy

- **Unit tests**: Buffer operations (insert, delete, undo, redo), cursor movement,
  wiki link parsing, front matter parsing, graph link extraction, search ranking.
- **Integration tests**: Create temp vault with markdown files, test full workflow
  (scan, index, search, read, edit, save, re-index).
- **Property tests**: Rope operations with proptest (random insert/delete sequences
  should never corrupt buffer state).
- **Visual tests**: GPU rendering via garasu screenshot comparison.

## Error Handling

- Module-specific error enums with `thiserror` (editor, storage, search, graph, sync).
- Top-level uses `anyhow::Result` for CLI error reporting.
- Graceful degradation: corrupt notes render as plain text, missing index triggers
  rebuild, broken wiki links show as red text.
- Auto-save failure: retry with exponential backoff, notify user via tsuuchi.
- Tracing: `tracing::{info,debug,warn,error}` with structured fields throughout.
