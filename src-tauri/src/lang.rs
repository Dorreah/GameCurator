//! Interface language of the few texts Windows shows for Game Curator
//! itself: file dialog titles, titles of the extra windows, and the error
//! boxes at start-up. Everything inside the windows is translated by the UI
//! (ui/js/i18n.js); this only follows the same setting (config.json).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static CZECH: AtomicBool = AtomicBool::new(false);

/// Follows the language setting ("en" | "cs").
pub fn set(language: &str) {
    CZECH.store(language == "cs", Ordering::Relaxed);
}

/// True when the interface is in Czech.
pub fn czech() -> bool {
    CZECH.load(Ordering::Relaxed)
}

/// Reads the language from `Data/config.json` before the archive is opened,
/// so that even the start-up error boxes speak the chosen language.
pub fn init_from_archive(root: &Path) {
    let Ok(text) = std::fs::read_to_string(root.join("Data").join("config.json")) else { return };
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(l) = v.get("language").and_then(|l| l.as_str()) {
            set(l);
        }
    }
}

/// The Czech version of a fixed English text (or the text itself).
pub fn tr(en: &'static str) -> &'static str {
    if !czech() {
        return en;
    }
    match en {
        "Choose a folder" => "Vyber složku",
        "Choose artwork (JPG, PNG or WebP)" => "Vyber obal (JPG, PNG nebo WebP)",
        "Choose the game's program (.exe)" => "Vyber program hry (.exe)",
        "Add screenshots (PNG, JPG or WebP)" => "Přidat screenshoty (PNG, JPG nebo WebP)",
        "Add a file to the game (fix, patch, mod, bonus material...)" => "Přidat ke hře soubor (fix, patch, mod, bonusový materiál…)",
        "Music" => "Hudba",
        "Back from the game \u{2014} Game Curator" => "Zpátky ze hry \u{2014} Game Curator",
        _ => en,
    }
}

/// Error box: the archive could not be opened.
pub fn archive_open_failed(location: &str, error: &str) -> String {
    if czech() {
        format!(
            "Archiv se nepodařilo otevřít.\n\nUmístění: {location}\n\n{error}\n\nPokud je disk chráněný proti zápisu, ochranu vypni a spusť Game Curator znovu."
        )
    } else {
        format!(
            "The archive could not be opened.\n\nLocation: {location}\n\n{error}\n\nIf the drive is write-protected, remove the protection and start Game Curator again."
        )
    }
}

/// Error box: no WebView2 runtime.
pub fn webview_missing() -> String {
    if czech() {
        "Game Curator potřebuje Microsoft Edge WebView2 Runtime, který na tomto počítači nebyl nalezen.\n\n\
         Windows 11 a aktualizované Windows 10 ho obsahují. Jinak nainstaluj „Evergreen“ WebView2 Runtime \
         od Microsoftu (hledej „WebView2 Runtime download“), nebo zkopíruj Fixed Version runtime do \
         složky „WebView2Runtime“ vedle GameCurator.exe.\n\nŽádná data nebyla změněna."
            .to_string()
    } else {
        "Game Curator needs the Microsoft Edge WebView2 Runtime, which was not found on this computer.\n\n\
         Windows 11 and up-to-date Windows 10 include it. Otherwise install the \"Evergreen\" WebView2 Runtime \
         from Microsoft (search for \"WebView2 Runtime download\"), or copy a Fixed Version runtime into the \
         folder \"WebView2Runtime\" next to GameCurator.exe.\n\nNo data was changed."
            .to_string()
    }
}

/// Error box: the main window could not be created.
pub fn window_failed(error: &str) -> String {
    if czech() {
        format!("Okno aplikace se nepodařilo vytvořit.\n\n{error}")
    } else {
        format!("The application window could not be created.\n\n{error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_setting() {
        set("cs");
        assert_eq!(tr("Music"), "Hudba");
        assert_eq!(tr("Something else"), "Something else");
        assert!(webview_missing().contains("WebView2"));
        set("en");
        assert_eq!(tr("Music"), "Music");
        assert!(archive_open_failed("E:\\", "x").starts_with("The archive"));
    }
}
