//! Built-in PDF viewer and music player.
//!
//! "Open" on a PDF shows it in a viewer window, "Open" on a music file plays
//! the game's soundtrack in a small player window - no other program needed.
//! Both windows are pages of the app itself (`viewer.html`, `player.html`)
//! and use the same WebView profile on the archive drive as the main window.
//!
//! The pages never see a file path: Rust registers each file under a token
//! and the page reads the file in parts through `media_read(token, ...)`.
//! Only files resolved from the catalog get a token, and everything here only
//! reads.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use curator_core::media::{self, MediaKind};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::state::{err, lock, AppState};
use crate::system;

type CmdResult<T> = Result<T, String>;

pub const PLAYER: &str = "player";

struct MediaEntry {
    path: PathBuf,
    name: String,
    game: String,
    size: u64,
}

/// Files the viewer and the player may read, and what each window shows.
#[derive(Default)]
pub struct Viewers {
    entries: HashMap<String, MediaEntry>,
    /// viewer window label -> token of its document
    windows: HashMap<String, String>,
    playlist: Option<Playlist>,
    counter: u64,
}

impl Viewers {
    fn register(&mut self, path: &Path, name: &str, game: &str, size: u64) -> String {
        if let Some((token, e)) = self.entries.iter_mut().find(|(_, e)| e.path == path) {
            e.name = name.to_string();
            e.game = game.to_string();
            e.size = size;
            return token.clone();
        }
        self.counter += 1;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let token = format!("m{:x}{:08x}", self.counter, nanos);
        self.entries.insert(token.clone(), MediaEntry { path: path.to_path_buf(), name: name.to_string(), game: game.to_string(), size });
        token
    }

    /// Forgets files no window shows any more.
    fn prune(&mut self) {
        let mut keep: Vec<&String> = self.windows.values().collect();
        if let Some(p) = &self.playlist {
            keep.extend(p.tracks.iter().map(|t| &t.token));
        }
        let keep: std::collections::HashSet<String> = keep.into_iter().cloned().collect();
        self.entries.retain(|t, _| keep.contains(t));
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistTrack {
    token: String,
    name: String,
    size: u64,
    /// File extension ("mp3", "flac", ...), for the audio type.
    ext: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    game: String,
    tracks: Vec<PlaylistTrack>,
    /// Track to start with (the one the user opened).
    start: usize,
    /// Increases with every new playlist.
    seq: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    token: String,
    name: String,
    game: String,
    size: u64,
}

fn file_size(p: &Path) -> u64 {
    p.metadata().map(|m| m.len()).unwrap_or(0)
}

fn window_label(path: &Path) -> String {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    format!("pdf-{:016x}", h.finish())
}

fn bring_to_front(w: &WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
}

/// A window showing one of the app's own pages, with the same WebView
/// profile (on the archive drive) and switches as the main window.
pub fn window_builder<'a>(app: &'a AppHandle, label: &str, page: &str, title: &str) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let state = app.state::<AppState>();
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title(title)
        .data_directory(state.layout.webview_dir());
    #[cfg(windows)]
    let builder = builder.additional_browser_args(crate::WEBVIEW2_ARGS);
    builder
}

fn build_window(app: &AppHandle, label: &str, page: &str, title: &str, size: (f64, f64), min: (f64, f64)) -> CmdResult<WebviewWindow> {
    let builder = window_builder(app, label, page, title).inner_size(size.0, size.1).min_inner_size(min.0, min.1).center();
    let win = builder.build().map_err(|e| format!("The window could not be opened.\n\n{e}"))?;
    let handle = app.clone();
    let label = label.to_string();
    win.on_window_event(move |ev| {
        if let tauri::WindowEvent::Destroyed = ev {
            let st = handle.state::<AppState>();
            let mut v = lock(&st.viewers);
            v.windows.remove(&label);
            if label == PLAYER {
                v.playlist = None;
            }
            v.prune();
        }
    });
    Ok(win)
}

/// Shows a PDF in its own viewer window (the same file twice: the window
/// that already shows it comes to the front).
pub fn open_pdf(app: &AppHandle, path: &Path, name: &str, game: &str) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let label = window_label(path);
    {
        let mut v = lock(&state.viewers);
        let token = v.register(path, name, game, file_size(path));
        v.windows.insert(label.clone(), token);
    }
    if let Some(w) = app.get_webview_window(&label) {
        bring_to_front(&w);
        return Ok(());
    }
    let title = format!("{name} \u{2014} {game}");
    build_window(app, &label, "viewer.html", &title, (920.0, 820.0), (480.0, 360.0)).map(|_| ())
}

/// Plays `tracks` from `start` in the player window (one player; a new
/// soundtrack replaces the old one).
pub fn open_player(app: &AppHandle, tracks: Vec<media::Track>, start: usize, game: &str) -> CmdResult<()> {
    if tracks.is_empty() {
        return Err("There is no music to play.".to_string());
    }
    let state = app.state::<AppState>();
    let playlist = {
        let mut v = lock(&state.viewers);
        let tracks: Vec<PlaylistTrack> = tracks
            .iter()
            .map(|t| PlaylistTrack {
                token: v.register(&t.path, &t.name, game, t.size),
                name: t.name.clone(),
                size: t.size,
                ext: curator_core::files::extension(&t.path.to_string_lossy()),
            })
            .collect();
        let seq = v.playlist.as_ref().map_or(1, |p| p.seq + 1);
        let start = start.min(tracks.len() - 1);
        let p = Playlist { game: game.to_string(), tracks, start, seq };
        v.playlist = Some(p.clone());
        v.prune();
        p
    };
    let title = format!("{game} \u{2014} {}", crate::lang::tr("Music"));
    if let Some(w) = app.get_webview_window(PLAYER) {
        let _ = w.set_title(&title);
        w.emit_to(PLAYER, "gc-player-load", &playlist).map_err(|e| e.to_string())?;
        bring_to_front(&w);
        return Ok(());
    }
    build_window(app, PLAYER, "player.html", &title, (440.0, 600.0), (360.0, 420.0)).map(|_| ())
}

/// Opens a catalogued file in the viewer or the player when it is a PDF or
/// music. Returns what was opened, `None` for other files.
pub fn open_archive_file(app: &AppHandle, game_id: i64, file_id: i64, path: &Path, name: &str) -> CmdResult<Option<&'static str>> {
    let state = app.state::<AppState>();
    match media::kind_of(name) {
        Some(MediaKind::Pdf) => {
            let (title, game) = state.with_db(|c, _| Ok((media::file_title(c, game_id, file_id)?, media::game_title(c, game_id)?)))?;
            open_pdf(app, path, &title, &game)?;
            Ok(Some("viewer"))
        }
        Some(MediaKind::Audio) => {
            let (mut tracks, game, title) = state.with_db(|c, l| {
                Ok((media::game_tracks(c, l, game_id)?, media::game_title(c, game_id)?, media::file_title(c, game_id, file_id)?))
            })?;
            let start = match tracks.iter().position(|t| t.source == "archive" && t.id == file_id) {
                Some(i) => i,
                None => {
                    tracks.insert(0, media::Track { path: path.to_path_buf(), name: title, size: file_size(path), source: "archive", id: file_id });
                    0
                }
            };
            open_player(app, tracks, start, &game)?;
            Ok(Some("player"))
        }
        None => Ok(None),
    }
}

/// The same for an additional file.
pub fn open_addition_file(app: &AppHandle, id: i64, path: &Path) -> CmdResult<Option<&'static str>> {
    let state = app.state::<AppState>();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match media::kind_of(&name) {
        Some(MediaKind::Pdf) => {
            let (title, game) = state.with_db(|c, l| {
                let a = curator_core::additions::get(c, l, id)?;
                Ok((media::addition_title(&a), media::game_title(c, a.game_id)?))
            })?;
            open_pdf(app, path, &title, &game)?;
            Ok(Some("viewer"))
        }
        Some(MediaKind::Audio) => {
            let (mut tracks, game, title) = state.with_db(|c, l| {
                let a = curator_core::additions::get(c, l, id)?;
                Ok((media::game_tracks(c, l, a.game_id)?, media::game_title(c, a.game_id)?, media::addition_title(&a)))
            })?;
            let start = match tracks.iter().position(|t| t.source == "addition" && t.id == id) {
                Some(i) => i,
                None => {
                    tracks.insert(0, media::Track { path: path.to_path_buf(), name: title, size: file_size(path), source: "addition", id });
                    0
                }
            };
            open_player(app, tracks, start, &game)?;
            Ok(Some("player"))
        }
        None => Ok(None),
    }
}

// ---------------------------------------------------------------- commands for the windows

/// The document of the calling viewer window.
#[tauri::command]
pub async fn viewer_document(window: WebviewWindow, state: State<'_, AppState>) -> CmdResult<DocumentInfo> {
    let v = lock(&state.viewers);
    let token = v.windows.get(window.label()).ok_or("This window has no document.")?;
    let e = v.entries.get(token).ok_or("The document is no longer available.")?;
    Ok(DocumentInfo { token: token.clone(), name: e.name.clone(), game: e.game.clone(), size: e.size })
}

/// The current playlist (the player asks when it starts).
#[tauri::command]
pub async fn player_state(state: State<'_, AppState>) -> CmdResult<Option<Playlist>> {
    Ok(lock(&state.viewers).playlist.clone())
}

fn entry_path(state: &AppState, token: &str) -> CmdResult<PathBuf> {
    lock(&state.viewers).entries.get(token).map(|e| e.path.clone()).ok_or_else(|| "The file is no longer available.".to_string())
}

/// Part of a registered file, as raw bytes (an ArrayBuffer in the page).
#[tauri::command]
pub async fn media_read(state: State<'_, AppState>, token: String, offset: u64, length: u64) -> CmdResult<tauri::ipc::Response> {
    let path = entry_path(&state, &token)?;
    let bytes = media::read_range(&path, offset, length).map_err(|e| {
        if path.exists() {
            err(e)
        } else {
            "The file is no longer available. Is the drive connected?".to_string()
        }
    })?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// "Open in default app" in the viewer and the player.
#[tauri::command]
pub async fn media_open_external(state: State<'_, AppState>, token: String) -> CmdResult<()> {
    let path = entry_path(&state, &token)?;
    system::shell_open(&path)
}

/// Closes the viewers and the player together with the main window.
pub fn close_all(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if label != "main" {
            let _ = w.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_get_tokens_that_are_forgotten_with_their_windows() {
        let mut v = Viewers::default();
        let a = v.register(Path::new("/games/x/Manual.pdf"), "Manual", "X", 10);
        let again = v.register(Path::new("/games/x/Manual.pdf"), "Manual (EN)", "X", 10);
        assert_eq!(a, again, "the same file keeps its token");
        assert_eq!(v.entries[&a].name, "Manual (EN)");
        let b = v.register(Path::new("/games/x/01.mp3"), "01", "X", 5);
        assert_ne!(a, b);
        v.windows.insert("pdf-1".into(), a.clone());
        v.playlist = Some(Playlist {
            game: "X".into(),
            tracks: vec![PlaylistTrack { token: b.clone(), name: "01".into(), size: 5, ext: "mp3".into() }],
            start: 0,
            seq: 1,
        });
        v.prune();
        assert_eq!(v.entries.len(), 2, "both still shown");
        v.windows.clear();
        v.prune();
        assert!(!v.entries.contains_key(&a) && v.entries.contains_key(&b), "the closed viewer's file is forgotten");
        v.playlist = None;
        v.prune();
        assert!(v.entries.is_empty());
        assert!(window_label(Path::new("/a.pdf")).starts_with("pdf-") && window_label(Path::new("/a.pdf")) != window_label(Path::new("/b.pdf")));
    }
}
