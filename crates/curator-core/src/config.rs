//! User preferences stored in `Data/config.json`.
//!
//! The database keeps data that belongs to the catalog (archive id, backup
//! timestamps); `config.json` keeps UI preferences, so restoring an older
//! database backup does not reset the user's display settings.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// Visual theme only (layout and controls are identical):
    /// "classic" (VGA launcher) | "amber" (Terminal CRT Amber) | "matrix" (Matrix terminal)
    /// | "dos" (DOS Midnight: MS-DOS text mode, darker)
    pub theme: String,
    /// "grid" | "catalog"
    pub view_mode: String,
    /// "small" | "medium" | "large"
    pub card_size: String,
    /// "title" | "year" | "added" | "size"
    pub sort: String,
    /// Default folder for the Archive Scanner (stored portable).
    pub scan_root: Option<String>,
    pub scan_depth: u32,
    pub wizard_completed: bool,
    pub auto_backup: bool,
    pub auto_backup_keep: u32,
    /// Width of the left column of the game profile (game, notes, journal)
    /// in percent; the archive gets the rest. Set by dragging the divider.
    pub profile_split: f64,
    /// Show the welcome window when Game Curator starts.
    pub show_welcome: bool,
    /// Font of the Classic look: "pixel-operator" | "dogica" | "geist-pixel"
    /// | "roboto-slab" (the text-mode themes have their own).
    pub classic_font: String,
    /// After a game started with Play has ended, offer to write a journal entry.
    pub journal_prompt: bool,
    /// Open PDF manuals in Game Curator's viewer and music in its player
    /// (otherwise in the default Windows application).
    pub builtin_viewer: bool,
}

/// Default left column of the game profile, in percent (the archive gets
/// a bit more: 1 : 1.35).
pub const PROFILE_SPLIT_DEFAULT: f64 = 42.5;

fn clamp_split(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(25.0, 75.0)
    } else {
        PROFILE_SPLIT_DEFAULT
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            theme: "classic".into(),
            view_mode: "grid".into(),
            card_size: "medium".into(),
            sort: "title".into(),
            scan_root: None,
            scan_depth: 3,
            wizard_completed: false,
            auto_backup: true,
            auto_backup_keep: 10,
            profile_split: PROFILE_SPLIT_DEFAULT,
            show_welcome: true,
            classic_font: "pixel-operator".into(),
            journal_prompt: true,
            builtin_viewer: true,
        }
    }
}

impl AppConfig {
    pub fn validate(mut self) -> Result<Self> {
        let check = |v: &str, allowed: &[&str], what: &str| {
            if allowed.contains(&v) {
                Ok(())
            } else {
                Err(CoreError::invalid(format!("Invalid {what}: '{v}'.")))
            }
        };
        check(&self.theme, THEMES, "theme")?;
        check(&self.view_mode, &["grid", "catalog"], "view mode")?;
        check(&self.card_size, &["small", "medium", "large"], "card size")?;
        check(&self.sort, SORTS, "sort order")?;
        check(&self.classic_font, CLASSIC_FONTS, "font")?;
        self.scan_depth = self.scan_depth.clamp(1, 6);
        self.auto_backup_keep = self.auto_backup_keep.clamp(1, 100);
        self.profile_split = clamp_split(self.profile_split);
        Ok(self)
    }

    /// Lenient variant used when loading: values that are no longer valid
    /// (e.g. from an older version) fall back to their defaults instead of
    /// discarding the whole file.
    pub fn sanitize(mut self) -> Self {
        let d = AppConfig::default();
        if !THEMES.contains(&self.theme.as_str()) {
            self.theme = d.theme;
        }
        if !["grid", "catalog"].contains(&self.view_mode.as_str()) {
            self.view_mode = d.view_mode;
        }
        if !["small", "medium", "large"].contains(&self.card_size.as_str()) {
            self.card_size = d.card_size;
        }
        if !SORTS.contains(&self.sort.as_str()) {
            self.sort = d.sort;
        }
        if !CLASSIC_FONTS.contains(&self.classic_font.as_str()) {
            self.classic_font = d.classic_font;
        }
        self.scan_depth = self.scan_depth.clamp(1, 6);
        self.auto_backup_keep = self.auto_backup_keep.clamp(1, 100);
        self.profile_split = clamp_split(self.profile_split);
        self
    }
}

const THEMES: &[&str] = &["classic", "amber", "matrix", "dos"];
const SORTS: &[&str] = &["title", "year", "added", "size"];
const CLASSIC_FONTS: &[&str] = &["pixel-operator", "dogica", "geist-pixel", "roboto-slab"];

/// Loads the configuration. A missing file yields defaults; a damaged file
/// is kept as `config.json.bad` and defaults are used.
pub fn load(path: &Path) -> AppConfig {
    match fs::read_to_string(path) {
        Ok(s) => match serde_json::from_str::<AppConfig>(&s) {
            Ok(c) => c.sanitize(),
            Err(_) => {
                let _ = fs::copy(path, path.with_extension("json.bad"));
                AppConfig::default()
            }
        },
        Err(_) => AppConfig::default(),
    }
}

pub fn save(path: &Path, cfg: &AppConfig) -> Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
