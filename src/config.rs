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
}

impl Default for HikkiConfig {
    fn default() -> Self {
        Self {
            appearance: AppearanceConfig::default(),
            editor: EditorConfig::default(),
            storage: StorageConfig::default(),
            search: SearchConfig::default(),
            sync: SyncConfig::default(),
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
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            font_size: 15.0,
            opacity: 0.95,
            line_spacing: 1.5,
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
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            tab_size: 4,
            word_wrap: true,
            spell_check: true,
            auto_save_secs: 30,
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
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            notes_dir: dirs::document_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")))
                .join("hikki"),
            format: "markdown".into(),
            auto_backup: true,
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
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            index_on_save: true,
            max_results: 50,
        }
    }
}

/// Sync configuration.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct SyncConfig {
    /// Enable note synchronization.
    pub enable: bool,
    /// Sync method: "git" or "icloud".
    pub method: String,
    /// Remote URL for git sync.
    pub remote_url: Option<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enable: false,
            method: "git".into(),
            remote_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = HikkiConfig::default();
        assert_eq!(config.appearance.width, 800);
        assert_eq!(config.editor.tab_size, 4);
        assert!(config.editor.word_wrap);
        assert!(!config.sync.enable);
    }

    #[test]
    fn storage_format_is_markdown() {
        let config = HikkiConfig::default();
        assert_eq!(config.storage.format, "markdown");
    }
}
