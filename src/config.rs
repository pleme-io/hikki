//! Hikki configuration — uses shikumi for discovery and hot-reload.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level configuration.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct HikkiConfig {
    pub appearance: AppearanceConfig,
    pub editor: EditorConfig,
    pub storage: StorageConfig,
    pub search: SearchConfig,
    pub sync: SyncConfig,
    pub preview: PreviewConfig,
}

impl Default for HikkiConfig {
    fn default() -> Self {
        Self {
            appearance: AppearanceConfig::default(),
            editor: EditorConfig::default(),
            storage: StorageConfig::default(),
            search: SearchConfig::default(),
            sync: SyncConfig::default(),
            preview: PreviewConfig::default(),
        }
    }
}

/// Visual appearance settings.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct AppearanceConfig {
    /// Window width in pixels.
    pub width: u32,
    /// Window height in pixels.
    pub height: u32,
    /// Font size in points.
    pub font_size: f32,
    /// Background opacity (0.0-1.0).
    pub opacity: f32,
    /// Line spacing multiplier.
    pub line_spacing: f32,
    /// Show line numbers.
    pub line_numbers: bool,
    /// Cursor blink.
    pub cursor_blink: bool,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            font_size: 15.0,
            opacity: 0.95,
            line_spacing: 1.5,
            line_numbers: true,
            cursor_blink: true,
        }
    }
}

/// Editor behavior settings.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct EditorConfig {
    /// Tab size in spaces.
    pub tab_size: u32,
    /// Enable word wrap.
    pub word_wrap: bool,
    /// Enable spell checking.
    pub spell_check: bool,
    /// Auto-save interval in seconds (0 to disable).
    pub auto_save_secs: u32,
    /// Auto-close brackets and quotes.
    pub auto_pairs: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            tab_size: 4,
            word_wrap: true,
            spell_check: false,
            auto_save_secs: 30,
            auto_pairs: true,
        }
    }
}

/// Note storage settings.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct StorageConfig {
    /// Directory where notes are stored.
    pub notes_dir: PathBuf,
    /// Note file format.
    pub format: String,
    /// Enable automatic backups.
    pub auto_backup: bool,
    /// Daily notes subdirectory (relative to notes_dir).
    pub daily_dir: String,
    /// Generate front matter on new notes.
    pub front_matter: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            notes_dir: dirs::document_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")))
                .join("hikki"),
            format: "markdown".into(),
            auto_backup: true,
            daily_dir: "daily".into(),
            front_matter: true,
        }
    }
}

/// Search / indexing configuration.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct SearchConfig {
    /// Re-index notes on every save.
    pub index_on_save: bool,
    /// Maximum search results to return.
    pub max_results: u32,
    /// Fuzzy matching threshold (0.0 = exact, 1.0 = very fuzzy).
    pub fuzzy_threshold: f32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            index_on_save: true,
            max_results: 50,
            fuzzy_threshold: 0.3,
        }
    }
}

/// Preview panel configuration.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct PreviewConfig {
    /// Show preview panel on startup.
    pub enabled: bool,
    /// Preview position: "right" or "bottom".
    pub position: String,
    /// Width ratio (fraction of window for preview).
    pub width_ratio: f32,
    /// Synchronize scroll between editor and preview.
    pub sync_scroll: bool,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: "right".into(),
            width_ratio: 0.5,
            sync_scroll: true,
        }
    }
}

/// Sync configuration.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct SyncConfig {
    /// Enable note synchronization.
    pub enable: bool,
    /// Sync method: "git" or "webdav".
    pub method: String,
    /// Remote URL for git sync.
    pub remote_url: Option<String>,
    /// Auto-commit on save.
    pub auto_commit: bool,
    /// Commit message for auto-commits.
    pub commit_message: String,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enable: false,
            method: "git".into(),
            remote_url: None,
            auto_commit: true,
            commit_message: "hikki: auto-save".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = HikkiConfig::default();
        assert_eq!(config.appearance.width, 1280);
        assert_eq!(config.appearance.height, 720);
        assert_eq!(config.editor.tab_size, 4);
        assert!(config.editor.word_wrap);
        assert!(!config.sync.enable);
        assert!(!config.preview.enabled);
        assert!(config.appearance.line_numbers);
    }

    #[test]
    fn storage_format_is_markdown() {
        let config = HikkiConfig::default();
        assert_eq!(config.storage.format, "markdown");
    }

    #[test]
    fn search_defaults() {
        let config = HikkiConfig::default();
        assert!(config.search.index_on_save);
        assert_eq!(config.search.max_results, 50);
    }

    #[test]
    fn preview_defaults() {
        let config = HikkiConfig::default();
        assert!(!config.preview.enabled);
        assert_eq!(config.preview.position, "right");
    }

    #[test]
    fn sync_defaults() {
        let config = HikkiConfig::default();
        assert!(!config.sync.enable);
        assert_eq!(config.sync.method, "git");
        assert!(config.sync.auto_commit);
    }

    #[test]
    fn storage_daily_dir() {
        let config = HikkiConfig::default();
        assert_eq!(config.storage.daily_dir, "daily");
        assert!(config.storage.front_matter);
    }
}
