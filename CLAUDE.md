# Hikki (筆記) — GPU Notes App

## Build & Test

```bash
cargo build                    # compile
cargo test --lib               # unit tests
cargo run                      # launch GUI
cargo run -- search "query"    # search notes from CLI
```

## Architecture

### Pipeline

```
Filesystem → Note Scanner → sakuin Index
                                  |
  Input Event → Editor → Markdown Parse → GPU Render
```

### Platform Isolation (`src/platform/`)

| Trait | macOS Impl | Purpose |
|-------|------------|---------|
| `NoteStorage` | `MacOSNoteStorage` | List, read, save, delete, search notes |

Linux implementations will be added under `src/platform/linux/`.

### Configuration

Uses **shikumi** for config discovery and hot-reload:
- Config file: `~/.config/hikki/hikki.yaml`
- Env override: `$HIKKI_CONFIG`
- Env vars: `HIKKI_` prefix (e.g. `HIKKI_EDITOR__TAB_SIZE=2`)
- Hot-reload on file change (nix-darwin symlink aware)

## File Map

| Path | Purpose |
|------|---------|
| `src/config.rs` | Config struct (uses shikumi) |
| `src/platform/mod.rs` | Platform trait definitions (NoteStorage, NoteMeta, Note) |
| `src/platform/macos/mod.rs` | macOS filesystem-backed note storage |
| `src/main.rs` | CLI entry point (GUI + search subcommands) |
| `src/lib.rs` | Library root |
| `module/default.nix` | HM module with typed options + sync daemon |

## Design Decisions

### Configuration Language: YAML
- YAML is the primary and only configuration format
- Config file: `~/.config/hikki/hikki.yaml`
- Nix HM module generates YAML via `lib.generators.toYAML` from typed options
- Typed options mirror `HikkiConfig` struct: appearance, editor, storage, search, sync
- `extraSettings` escape hatch for raw attrset merge on top of typed options

### Note Format: Markdown
- Notes stored as `.md` files in configurable `notes_dir`
- Title extracted from first `# heading` or first non-empty line
- Tags extracted from `tags:` line in content
- Markdown rendering via pulldown-cmark

### Nix Integration
- Flake exports: `packages`, `overlays.default`, `homeManagerModules.default`, `devShells`
- HM module at `blackmatter.components.hikki` with fully typed options:
  - `appearance.{width, height, font_size, opacity, line_spacing}`
  - `editor.{tab_size, word_wrap, spell_check, auto_save_secs}`
  - `storage.{notes_dir, format, auto_backup}`
  - `search.{index_on_save, max_results}`
  - `sync.{enable, method, remote_url}` with launchd/systemd service
  - `extraSettings` — raw attrset escape hatch
- YAML generated via `lib.generators.toYAML` -> `xdg.configFile."hikki/hikki.yaml"`
- Cross-platform: `mkLaunchdService` (macOS) + `mkSystemdService` (Linux) for sync
- Uses substrate's `hm-service-helpers.nix` for service generation

### Cross-Platform Strategy
- Platform-specific: behind trait boundaries in `src/platform/`
- Search index: sakuin (tantivy wrapper) for note metadata
- Markdown parsing: pulldown-cmark (cross-platform)
- Config: shikumi for discovery and hot-reload
