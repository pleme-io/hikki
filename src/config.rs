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
    /// Fleet visual theme. Drives every render color through ishou
    /// design tokens (`ishou_tokens::FleetTheme::resolve()`) instead
    /// of a hand-authored palette. `PlemeDark` is the prescribed
    /// default; flipping this here (or via `HIKKI_TIER=bare`) reaches
    /// the GPU renderer's palette on the next launch, so a fleet
    /// theme switch propagates to hikki instead of silently diverging.
    pub theme: ishou_tokens::FleetTheme,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        // `font_size` + `theme` are pulled from the fleet baseline so
        // a `FleetDefaults` change propagates here on recompile (the
        // convergence-by-construction guarantee — see the guard test
        // below). Layout fields (width/height/opacity/line_spacing)
        // are hikki-specific and stay local.
        let fd = ishou_tokens::FleetDefaults::prescribed();
        Self {
            width: 1280,
            height: 720,
            font_size: fd.font_size,
            opacity: 0.95,
            line_spacing: 1.5,
            line_numbers: true,
            cursor_blink: true,
            theme: fd.theme,
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

// ─────────────────────────────────────────────────────────────────
// Configuration prime directive — shikumi::TieredConfig
// ─────────────────────────────────────────────────────────────────
//
// Per pleme-io's configuration prime directive (shikumi@27d1a12,
// 2026-05-20), every typed config exposes bare/discovered/
// prescribed_default tiers via the shikumi::TieredConfig trait.
// Operators reach via:
//
//   HIKKI_TIER=bare hikki ...
//   HIKKI_TIER=default hikki ...
//
// `bare()` = zero-opinion floor (empty strings/paths, all toggles off).
// `prescribed_default()` = curated defaults shipped today (Default impl).

impl shikumi::TieredConfig for HikkiConfig {
    fn bare() -> Self {
        Self {
            appearance: <AppearanceConfig as shikumi::TieredConfig>::bare(),
            editor: <EditorConfig as shikumi::TieredConfig>::bare(),
            storage: <StorageConfig as shikumi::TieredConfig>::bare(),
            search: <SearchConfig as shikumi::TieredConfig>::bare(),
            sync: <SyncConfig as shikumi::TieredConfig>::bare(),
            preview: <PreviewConfig as shikumi::TieredConfig>::bare(),
        }
    }

    fn prescribed_default() -> Self {
        Self::default()
    }
}

impl shikumi::TieredConfig for AppearanceConfig {
    fn bare() -> Self {
        Self {
            width: 0,
            height: 0,
            font_size: 0.0,
            opacity: 0.0,
            line_spacing: 0.0,
            line_numbers: false,
            cursor_blink: false,
            theme: ishou_tokens::FleetTheme::bare(),
        }
    }

    fn prescribed_default() -> Self {
        Self::default()
    }
}

/// Convergence-by-construction: the visual fields hikki shares with
/// the fleet baseline (`theme`, `font_size`) are materialized from
/// `FleetDefaults` rather than hand-copied. App-specific layout fields
/// fall back to the zero-opinion `bare()` floor via struct-update.
impl ishou_tokens::FleetThemedConfig for AppearanceConfig {
    fn from_fleet(fd: &ishou_tokens::FleetDefaults) -> Self {
        Self {
            theme: fd.theme,
            font_size: fd.font_size,
            ..<Self as shikumi::TieredConfig>::bare()
        }
    }
}

impl shikumi::TieredConfig for EditorConfig {
    fn bare() -> Self {
        Self {
            tab_size: 0,
            word_wrap: false,
            spell_check: false,
            auto_save_secs: 0,
            auto_pairs: false,
        }
    }

    fn prescribed_default() -> Self {
        Self::default()
    }
}

impl shikumi::TieredConfig for StorageConfig {
    fn bare() -> Self {
        Self {
            notes_dir: PathBuf::new(),
            format: String::new(),
            auto_backup: false,
            daily_dir: String::new(),
            front_matter: false,
        }
    }

    fn prescribed_default() -> Self {
        Self::default()
    }
}

impl shikumi::TieredConfig for SearchConfig {
    fn bare() -> Self {
        Self {
            index_on_save: false,
            max_results: 0,
            fuzzy_threshold: 0.0,
        }
    }

    fn prescribed_default() -> Self {
        Self::default()
    }
}

impl shikumi::TieredConfig for PreviewConfig {
    fn bare() -> Self {
        Self {
            enabled: false,
            position: String::new(),
            width_ratio: 0.0,
            sync_scroll: false,
        }
    }

    fn prescribed_default() -> Self {
        Self::default()
    }
}

impl shikumi::TieredConfig for SyncConfig {
    fn bare() -> Self {
        Self {
            enable: false,
            method: String::new(),
            remote_url: None,
            auto_commit: false,
            commit_message: String::new(),
        }
    }

    fn prescribed_default() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tiered_tests {
    use super::*;
    use shikumi::{ConfigTier, TieredConfig};

    #[test]
    fn bare_is_zero_opinion() {
        let b = <HikkiConfig as TieredConfig>::bare();
        assert_eq!(b.appearance.width, 0);
        assert_eq!(b.appearance.height, 0);
        assert_eq!(b.editor.tab_size, 0);
        assert!(!b.editor.word_wrap);
        assert!(b.storage.notes_dir.as_os_str().is_empty());
        assert!(b.storage.format.is_empty());
        assert_eq!(b.search.max_results, 0);
        assert!(!b.sync.enable);
        assert!(!b.preview.enabled);
    }

    #[test]
    fn prescribed_matches_default() {
        let p = <HikkiConfig as TieredConfig>::prescribed_default();
        let d = HikkiConfig::default();
        assert_eq!(p.appearance.width, d.appearance.width);
        assert_eq!(p.editor.tab_size, d.editor.tab_size);
        assert_eq!(p.storage.format, d.storage.format);
        assert_eq!(p.sync.method, d.sync.method);
    }

    #[test]
    fn diff_bare_vs_default_is_non_empty() {
        let b = <HikkiConfig as TieredConfig>::bare();
        let d = <HikkiConfig as TieredConfig>::prescribed_default();
        let diff = d.diff_against(&b);
        assert!(
            !diff.is_empty_diff(),
            "bare and prescribed_default must differ"
        );
    }

    #[test]
    fn resolve_tier_dispatches() {
        assert_eq!(
            <HikkiConfig as TieredConfig>::resolve_tier(ConfigTier::Bare)
                .appearance
                .width,
            0
        );
        assert!(
            <HikkiConfig as TieredConfig>::resolve_tier(ConfigTier::Default)
                .appearance
                .width
                > 0
        );
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

// ── Fleet convergence guard ──────────────────────────────────────
//
// Every visual default above MUST match the corresponding field on
// `ishou_tokens::FleetDefaults::prescribed()`. If they drift,
// hikki's defaults silently diverge from the rest of the fleet
// (mado, escriba, namimado, hibiki, fumi, etc.). The test below
// enforces convergence at compile-time-of-the-test-suite.

#[cfg(test)]
mod fleet_convergence_tests {
    use super::*;

    /// One-line convergence guard — pinned to
    /// `ishou_tokens::convergence::Guard` (ishou@1cfd3cf). Drift on
    /// any default visual field surfaces as a single panic listing
    /// every field that diverged. Pattern reused from mado@cdded35.
    #[test]
    fn fallback_defaults_converge_with_fleet() {
        let appearance = AppearanceConfig::default();
        ishou_tokens::convergence::Guard::for_app("hikki")
            .expect_font_size(appearance.font_size)
            .expect_theme(appearance.theme)
            .run();
    }

    /// `from_fleet` is the FleetThemedConfig factory — materializing
    /// from the prescribed baseline must yield the canonical fleet
    /// theme + font size (proves the convergence path is live).
    #[test]
    fn from_fleet_pulls_canonical_visuals() {
        use ishou_tokens::{FleetDefaults, FleetTheme, FleetThemedConfig};
        let fd = FleetDefaults::prescribed();
        let a = AppearanceConfig::from_fleet(&fd);
        assert_eq!(a.theme, FleetTheme::PlemeDark);
        assert_eq!(a.font_size, fd.font_size);
    }
}
