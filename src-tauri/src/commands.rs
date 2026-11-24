//! Tauri commands: thin wrappers around `curator_core`. Every command runs
//! off the UI thread (`async`), validates its input in the core and returns
//! user-readable error messages.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use curator_core::archive::{self, ArchiveIdStatus};
use curator_core::backup::{self, BackupInfo, RestoreReport};
use curator_core::config::{self, AppConfig};
use curator_core::files::{self, ArchiveContent};
use curator_core::games::{
    self, AvailabilityReport, GameCard, GameDetail, GameInput, GamePatch, LibraryFacets, LibraryQuery, RemovalPreview,
    RemovalSummary,
};
use curator_core::icons;
use curator_core::images::{self, Crop, ProcessedArtwork};
use curator_core::installs::{self, DetectReport, InstallStatus};
use curator_core::journal::{self, JournalEntry, JournalInput, JournalQuery};
use curator_core::scanner::{self, ImportItem, ImportReport, ScanResult};
use curator_core::additions::{self, Addition, AdditionInput};
use curator_core::screenshots::{self, Screenshot};
use curator_core::{paths, CoreError};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::state::{err, lock, AppState};
use crate::system;

type CmdResult<T> = Result<T, String>;

// ---------------------------------------------------------------- app / archive

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
    archive_root: String,
    data_dir: String,
    db_path: String,
    archive_id: String,
    id_status: ArchiveIdStatus,
    db_created: bool,
    webview_version: String,
    platform: &'static str,
    drive_kind: String,
    game_count: i64,
    writable: bool,
    games_dir: String,
    startup_notes: Vec<String>,
    test_mode: bool,
    machine: String,
}

#[tauri::command]
pub async fn app_info(state: State<'_, AppState>) -> CmdResult<AppInfo> {
    let game_count = state.with_db(|c, _| games::game_count(c))?;
    let l = &state.layout;
    Ok(AppInfo {
        name: curator_core::APP_NAME,
        version: curator_core::APP_VERSION,
        archive_root: paths::display(&l.root),
        data_dir: paths::display(&l.data_dir()),
        db_path: paths::display(&l.db_path()),
        archive_id: lock(&state.archive_id).clone(),
        id_status: lock(&state.id_status).clone(),
        db_created: state.db_created,
        webview_version: state.webview_version.clone(),
        platform: std::env::consts::OS,
        drive_kind: system::drive_kind(&l.root),
        game_count,
        writable: l.check_writable().is_ok(),
        games_dir: paths::display(&l.games_dir()),
        startup_notes: state.startup_notes.clone(),
        test_mode: cfg!(feature = "e2e"),
        machine: state.machine.clone(),
    })
}

/// Resolves an archive id mismatch by assigning the database to this archive.
#[tauri::command]
pub async fn adopt_database(state: State<'_, AppState>) -> CmdResult<()> {
    let id = {
        let mut db = lock(&state.db);
        archive::adopt_database(&state.layout, &mut db).map_err(err)?
    };
    *lock(&state.archive_id) = id;
    *lock(&state.id_status) = ArchiveIdStatus::Ok;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStatus {
    available: bool,
    root: String,
}

#[tauri::command]
pub async fn archive_status(state: State<'_, AppState>) -> CmdResult<ArchiveStatus> {
    let available = state.layout.is_available();
    if !available {
        lock(&state.db).invalidate();
    }
    Ok(ArchiveStatus { available, root: paths::display(&state.layout.root) })
}

// ---------------------------------------------------------------- config

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> CmdResult<AppConfig> {
    let mut cfg = lock(&state.config).clone();
    cfg.scan_root = cfg.scan_root.map(|p| paths::display(&paths::resolve(&state.layout.root, &p)));
    Ok(cfg)
}

/// Saves preferences. `scanRoot` is received as an absolute path and stored
/// relative to the archive when possible.
#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, config: AppConfig) -> CmdResult<AppConfig> {
    if !state.layout.is_available() {
        return Err(CoreError::ArchiveUnavailable.to_string());
    }
    let mut cfg = config.validate().map_err(err)?;
    cfg.scan_root = cfg
        .scan_root
        .filter(|p| !p.trim().is_empty())
        .map(|p| paths::to_portable(&state.layout.root, &paths::resolve(&state.layout.root, p.trim())));
    config::save(&state.layout.config_path(), &cfg).map_err(err)?;
    crate::lang::set(&cfg.language);
    *lock(&state.config) = cfg.clone();
    cfg.scan_root = cfg.scan_root.map(|p| paths::display(&paths::resolve(&state.layout.root, &p)));
    Ok(cfg)
}

// ---------------------------------------------------------------- library

#[tauri::command]
pub async fn list_games(state: State<'_, AppState>, query: LibraryQuery) -> CmdResult<Vec<GameCard>> {
    state.with_db(|c, _| games::list_games(c, &query, &state.machine))
}

#[tauri::command]
pub async fn library_facets(state: State<'_, AppState>) -> CmdResult<LibraryFacets> {
    state.with_db(|c, _| games::library_facets(c))
}

#[derive(Serialize)]
pub struct GameTitle {
    id: i64,
    title: String,
}

#[tauri::command]
pub async fn game_titles(state: State<'_, AppState>) -> CmdResult<Vec<GameTitle>> {
    state.with_db(|c, _| Ok(games::list_titles(c)?.into_iter().map(|(id, title)| GameTitle { id, title }).collect()))
}

#[tauri::command]
pub async fn get_game(state: State<'_, AppState>, id: i64) -> CmdResult<GameDetail> {
    state.with_db(|c, l| games::get_game(c, l, id))
}

#[tauri::command]
pub async fn create_game(state: State<'_, AppState>, input: GameInput) -> CmdResult<i64> {
    state.with_db(|c, l| {
        let id = games::create_game(c, l, &input)?;
        // index the folder right away so the profile opens instantly
        let folder = paths::resolve(&l.root, &input.folder_path);
        if let Ok(listing) = files::list_folder(&folder, files::MAX_DEPTH, files::MAX_ENTRIES) {
            files::sync_files(c, id, &listing)?;
            if let Some(p) = files::guess_primary(&listing.files) {
                files::apply_guessed_primary(c, id, &p)?;
            }
        }
        Ok(id)
    })
}

#[tauri::command]
pub async fn update_game(state: State<'_, AppState>, id: i64, input: GameInput) -> CmdResult<()> {
    state.with_db(|c, l| games::update_game(c, l, id, &input))
}

#[tauri::command]
pub async fn patch_game(state: State<'_, AppState>, id: i64, patch: GamePatch) -> CmdResult<()> {
    state.with_db(|c, _| games::patch_game(c, id, &patch))
}

#[tauri::command]
pub async fn removal_preview(state: State<'_, AppState>, id: i64) -> CmdResult<RemovalPreview> {
    state.with_db(|c, l| games::removal_preview(c, l, id))
}

/// Removes a game from the catalog after confirmation. The folder on disk is
/// never touched. Journal entries are removed from the database; when
/// `keepJournalCopies` is true their HTML copies stay in `Data/Journals`.
#[tauri::command]
pub async fn remove_game(
    state: State<'_, AppState>,
    id: i64,
    keep_journal_copies: bool,
    keep_screenshots: Option<bool>,
    keep_additions: Option<bool>,
) -> CmdResult<RemovalSummary> {
    let opts = games::RemoveOptions {
        delete_screenshots: !keep_screenshots.unwrap_or(true),
        delete_addition_copies: !keep_additions.unwrap_or(true),
    };
    state.with_db(|c, l| {
        let entries = journal::list_entries(c, &JournalQuery { game_id: Some(id), ..Default::default() })?;
        if keep_journal_copies {
            for e in &entries {
                journal::write_mirror(l, e)?;
            }
        }
        let summary = games::remove_game_with(c, l, id, &opts)?;
        if !keep_journal_copies {
            journal::remove_game_mirrors(l, id);
        }
        Ok(summary)
    })
}

#[tauri::command]
pub async fn check_availability(state: State<'_, AppState>) -> CmdResult<AvailabilityReport> {
    state.with_db(|c, l| games::check_availability(c, &l.root))
}

// ---------------------------------------------------------------- dialogs

/// In end-to-end test builds dialogs are replaced by a file containing the
/// path to "pick" (GC_E2E_PICK_FILE). Returns `None` in normal builds.
#[allow(unused_variables)]
fn e2e_pick() -> Option<Option<PathBuf>> {
    #[cfg(feature = "e2e")]
    if let Some(f) = std::env::var_os("GC_E2E_PICK_FILE") {
        let s = std::fs::read_to_string(f).unwrap_or_default();
        let s = s.trim();
        return Some(if s.is_empty() { None } else { Some(PathBuf::from(s)) });
    }
    None
}

/// Several files from one dialog (E2E: one path per line).
fn e2e_pick_many() -> Option<Vec<PathBuf>> {
    #[cfg(feature = "e2e")]
    if let Some(f) = std::env::var_os("GC_E2E_PICK_FILE") {
        let s = std::fs::read_to_string(f).unwrap_or_default();
        return Some(s.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from).collect());
    }
    None
}

#[tauri::command]
pub async fn pick_folder(
    window: WebviewWindow,
    state: State<'_, AppState>,
    title: Option<String>,
    start: Option<String>,
) -> CmdResult<Option<String>> {
    if let Some(p) = e2e_pick() {
        return Ok(p.map(|p| paths::display(&p)));
    }
    let mut dlg = window.app_handle().dialog().file().set_parent(&window).set_title(title.unwrap_or_else(|| crate::lang::tr("Choose a folder").into()));
    let start_dir = start
        .filter(|s| !s.trim().is_empty())
        .map(|s| paths::resolve(&state.layout.root, s.trim()))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| state.layout.root.clone());
    dlg = dlg.set_directory(start_dir);
    let picked = dlg.blocking_pick_folder();
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| paths::display(&p)))
}

/// Lets the user choose an image and converts it to optimized WebP files in
/// the archive. Returns `None` when the dialog was cancelled.
#[tauri::command]
pub async fn pick_artwork(window: WebviewWindow, state: State<'_, AppState>) -> CmdResult<Option<ProcessedArtwork>> {
    let path = match e2e_pick() {
        Some(p) => p,
        None => window
            .app_handle()
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(crate::lang::tr("Choose artwork (JPG, PNG or WebP)"))
            .add_filter("Images", images::ALLOWED_EXTENSIONS)
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok()),
    };
    let Some(path) = path else { return Ok(None) };
    if !state.layout.is_available() {
        return Err(CoreError::ArchiveUnavailable.to_string());
    }
    images::process_artwork(&state.layout, &path).map(Some).map_err(err)
}

/// Shows another part of the game's artwork in the card frame. Returns the
/// new thumbnail path.
#[tauri::command]
pub async fn set_cover_crop(state: State<'_, AppState>, id: i64, crop: Crop) -> CmdResult<String> {
    state.with_db(|c, l| games::set_cover_crop(c, l, id, &crop))
}

// ---------------------------------------------------------------- installed games

/// Matches the programs installed on this computer to the catalog (runs at
/// start and on request; reading the registry takes milliseconds).
#[tauri::command]
pub async fn detect_installs(state: State<'_, AppState>) -> CmdResult<DetectReport> {
    let programs = system::installed_programs();
    state.with_db(|c, l| {
        let report = installs::refresh(c, &state.machine, &programs)?;
        // icons for the catalog: read from the installations, never changed there;
        // a game whose icon cannot be read simply has none
        let _ = icons::refresh_icons(c, l, &state.machine);
        Ok(report)
    })
}

#[tauri::command]
pub async fn install_status(state: State<'_, AppState>, game_id: i64) -> CmdResult<Option<InstallStatus>> {
    state.with_db(|c, _| installs::status(c, &state.machine, game_id))
}

/// The user points to the game's program (.exe) on this computer.
#[tauri::command]
pub async fn locate_install(window: WebviewWindow, state: State<'_, AppState>, game_id: i64) -> CmdResult<Option<InstallStatus>> {
    let path = match e2e_pick() {
        Some(p) => p,
        None => window
            .app_handle()
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(crate::lang::tr("Choose the game's program (.exe)"))
            .add_filter("Programs", &["exe"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok()),
    };
    let Some(path) = path else { return Ok(None) };
    state
        .with_db(|c, l| {
            let status = installs::set_manual(c, &state.machine, game_id, &path)?;
            let _ = icons::update_icon(c, l, &state.machine, game_id, true);
            Ok(status)
        })
        .map(Some)
}

/// Forgets where the game is installed on this computer (the record only).
#[tauri::command]
pub async fn forget_install(state: State<'_, AppState>, game_id: i64) -> CmdResult<()> {
    state.with_db(|c, _| installs::forget(c, &state.machine, game_id))
}

#[tauri::command]
pub async fn open_install_folder(state: State<'_, AppState>, game_id: i64) -> CmdResult<()> {
    let dir = state.with_db(|c, _| installs::install_dir(c, &state.machine, game_id))?;
    system::shell_open(&dir)
}

/// Starts the installed game. The UI asks the user first; `confirmed` makes
/// that explicit at the API level. The program is resolved from the stored
/// record (never from a path sent by the UI) and must lie inside the
/// installation folder.
#[tauri::command]
pub async fn play_game(app: tauri::AppHandle, state: State<'_, AppState>, game_id: i64, confirmed: bool) -> CmdResult<()> {
    if !confirmed {
        return Err("Starting a game requires your confirmation.".to_string());
    }
    let (target, title, dir) = state.with_db(|c, _| {
        let target = installs::launch_target(c, &state.machine, game_id)?;
        let title: String = c.query_row("SELECT title FROM games WHERE id = ?1", [game_id], |r| r.get(0))?;
        let dir = installs::install_dir(c, &state.machine, game_id)?;
        Ok((target, title, dir))
    })?;
    let process = system::start_game(&target)?;
    // notice when the game ends, to offer a journal entry (only for games started here)
    crate::session::watch(&app, game_id, title, dir, process);
    Ok(())
}

/// Games started with Play that are still running.
#[tauri::command]
pub async fn running_games(state: State<'_, AppState>) -> CmdResult<Vec<i64>> {
    Ok(lock(&state.running_games).iter().copied().collect())
}

// ---------------------------------------------------------------- archive content

#[tauri::command]
pub async fn archive_content(state: State<'_, AppState>, game_id: i64) -> CmdResult<ArchiveContent> {
    state.with_db(|c, l| files::archive_content(c, l, game_id))
}

#[tauri::command]
pub async fn set_file_category(state: State<'_, AppState>, game_id: i64, file_id: i64, category: Option<String>) -> CmdResult<()> {
    state.with_db(|c, _| files::set_category(c, game_id, file_id, category.as_deref()))
}

/// The name the catalog shows for a file; the file on disk keeps its name.
#[tauri::command]
pub async fn set_file_display_name(state: State<'_, AppState>, game_id: i64, file_id: i64, name: String) -> CmdResult<Option<String>> {
    state.with_db(|c, _| files::set_display_name(c, game_id, file_id, &name))
}

#[tauri::command]
pub async fn set_primary_file(state: State<'_, AppState>, game_id: i64, file_id: Option<i64>) -> CmdResult<()> {
    state.with_db(|c, _| files::set_primary(c, game_id, file_id))
}

/// Opens a document/image/music file with its default application.
/// Programs and scripts are refused here (see `run_file`).
#[tauri::command]
pub async fn open_file(app: AppHandle, state: State<'_, AppState>, game_id: i64, file_id: i64) -> CmdResult<&'static str> {
    let (path, name) = state.with_db(|c, l| files::resolve_file(c, l, game_id, file_id))?;
    if files::is_risky(&name) {
        return Err(if files::is_runnable(&name) {
            "This is a program. Use \u{201c}Run\u{201d} to start it after confirmation.".to_string()
        } else {
            "For safety, Game Curator does not open script or shortcut files. Use \u{201c}Open Folder\u{201d} instead.".to_string()
        });
    }
    // PDF manuals and music open in Game Curator's own viewer / player
    if lock(&state.config).builtin_viewer {
        if let Some(opened) = crate::viewer::open_archive_file(&app, game_id, file_id, &path, &name)? {
            return Ok(opened);
        }
    }
    system::shell_open(&path)?;
    Ok("external")
}

/// Starts an `.exe` / `.msi` file. The UI must have asked the user first;
/// `confirmed` makes that explicit at the API level.
#[tauri::command]
pub async fn run_file(state: State<'_, AppState>, game_id: i64, file_id: i64, confirmed: bool) -> CmdResult<()> {
    if !confirmed {
        return Err("Running a program requires your confirmation.".to_string());
    }
    let (path, name) = state.with_db(|c, l| files::resolve_file(c, l, game_id, file_id))?;
    if !files::is_runnable(&name) {
        return Err("Only .exe and .msi files can be run.".to_string());
    }
    system::run_program(&path)
}

#[tauri::command]
pub async fn open_game_folder(state: State<'_, AppState>, game_id: i64, subfolder: Option<String>) -> CmdResult<()> {
    let path = state.with_db(|c, l| files::resolve_subfolder(c, l, game_id, subfolder.as_deref()))?;
    system::shell_open(&path)
}

// ---------------------------------------------------------------- journal

#[tauri::command]
pub async fn list_journal(state: State<'_, AppState>, query: JournalQuery) -> CmdResult<Vec<JournalEntry>> {
    state.with_db(|c, _| journal::list_entries(c, &query))
}

#[tauri::command]
pub async fn create_journal(state: State<'_, AppState>, input: JournalInput) -> CmdResult<JournalEntry> {
    state.with_db(|c, l| {
        let e = journal::create_entry(c, &input)?;
        if let Err(m) = journal::write_mirror(l, &e) {
            eprintln!("journal mirror not written: {m}");
        }
        Ok(e)
    })
}

#[tauri::command]
pub async fn update_journal(state: State<'_, AppState>, id: i64, input: JournalInput) -> CmdResult<JournalEntry> {
    state.with_db(|c, l| {
        let (e, previous_game) = journal::update_entry(c, id, &input)?;
        if previous_game != e.game_id {
            journal::remove_mirror(l, previous_game, id);
        }
        if let Err(m) = journal::write_mirror(l, &e) {
            eprintln!("journal mirror not written: {m}");
        }
        Ok(e)
    })
}

#[tauri::command]
pub async fn delete_journal(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    state.with_db(|c, l| {
        let e = journal::delete_entry(c, id)?;
        journal::remove_mirror(l, e.game_id, e.id);
        Ok(())
    })
}

/// Rewrites the HTML copies of journal entries (their screenshots changed).
fn refresh_mirrors(c: &curator_core::rusqlite::Connection, l: &archive::ArchiveLayout, entries: &[i64]) {
    for id in entries {
        if let Ok(e) = journal::get_entry(c, *id) {
            if let Err(m) = journal::write_mirror(l, &e) {
                eprintln!("journal mirror not written: {m}");
            }
        }
    }
}

// ---------------------------------------------------------------- screenshots

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedScreenshots {
    pub added: Vec<Screenshot>,
    /// "file name: reason" for pictures that could not be added.
    pub failed: Vec<String>,
}

/// Lets the user choose one or more pictures and adds them to the game's
/// screenshots (and to a journal entry, when given).
#[tauri::command]
pub async fn add_screenshots(
    window: WebviewWindow,
    state: State<'_, AppState>,
    game_id: i64,
    entry_id: Option<i64>,
) -> CmdResult<AddedScreenshots> {
    let picked = match e2e_pick_many() {
        Some(p) => p,
        None => window
            .app_handle()
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(crate::lang::tr("Add screenshots (PNG, JPG or WebP)"))
            .add_filter("Images", images::ALLOWED_EXTENSIONS)
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.into_path().ok())
            .collect(),
    };
    Ok(add_screenshot_files(&state, game_id, entry_id, &picked))
}

/// Adds dropped pictures (by their tokens) to the game's screenshots.
#[tauri::command]
pub async fn add_dropped_screenshots(
    state: State<'_, AppState>,
    game_id: i64,
    entry_id: Option<i64>,
    tokens: Vec<String>,
) -> CmdResult<AddedScreenshots> {
    let picked: Vec<PathBuf> = {
        let mut map = lock(&state.picked);
        tokens.iter().filter_map(|t| map.remove(t)).collect()
    };
    if picked.is_empty() {
        return Err("Please drop the pictures again.".to_string());
    }
    Ok(add_screenshot_files(&state, game_id, entry_id, &picked))
}

fn add_screenshot_files(state: &AppState, game_id: i64, entry_id: Option<i64>, picked: &[PathBuf]) -> AddedScreenshots {
    let mut out = AddedScreenshots { added: Vec::new(), failed: Vec::new() };
    for path in picked.iter().take(200) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match state.with_db(|c, l| screenshots::add(c, l, game_id, entry_id, path)) {
            Ok(s) => out.added.push(s),
            Err(e) => out.failed.push(format!("{name}: {e}")),
        }
    }
    if let (Some(e), false) = (entry_id, out.added.is_empty()) {
        let _ = state.with_db(|c, l| {
            refresh_mirrors(c, l, &[e]);
            Ok(())
        });
    }
    out
}

/// Test builds only: simulates files dropped from Explorer at (x, y).
#[tauri::command]
pub async fn e2e_drop(window: WebviewWindow, paths: Vec<String>, x: f64, y: f64) -> CmdResult<()> {
    if !cfg!(feature = "e2e") {
        return Err("Not available.".to_string());
    }
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    crate::dragdrop::dropped(&window, &paths, x, y);
    Ok(())
}

#[tauri::command]
pub async fn list_screenshots(state: State<'_, AppState>, game_id: i64) -> CmdResult<Vec<Screenshot>> {
    state.with_db(|c, _| screenshots::list_for_game(c, game_id))
}

#[tauri::command]
pub async fn set_screenshot_caption(state: State<'_, AppState>, id: i64, caption: String) -> CmdResult<Screenshot> {
    state.with_db(|c, l| {
        let s = screenshots::set_caption(c, id, &caption)?;
        refresh_mirrors(c, l, &s.journal_entry_id.into_iter().collect::<Vec<_>>());
        Ok(s)
    })
}

/// Attaches screenshots to a journal entry, or detaches them (`entryId` null).
#[tauri::command]
pub async fn attach_screenshots(state: State<'_, AppState>, ids: Vec<i64>, entry_id: Option<i64>) -> CmdResult<()> {
    state.with_db(|c, l| {
        let touched = screenshots::attach(c, &ids, entry_id)?;
        refresh_mirrors(c, l, &touched);
        Ok(())
    })
}

/// Deletes a screenshot copy (the UI asks first). The original is not touched.
#[tauri::command]
pub async fn delete_screenshot(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    state.with_db(|c, l| {
        let s = screenshots::delete(c, l, id)?;
        refresh_mirrors(c, l, &s.journal_entry_id.into_iter().collect::<Vec<_>>());
        Ok(())
    })
}

/// Opens the screenshot in the Windows photo viewer.
#[tauri::command]
pub async fn open_screenshot(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let p = state.with_db(|c, l| screenshots::image_file(c, l, id))?;
    system::shell_open(&p)
}

// ---------------------------------------------------------------- additional files

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    pub token: String,
    pub name: String,
    pub size: u64,
    pub location: String,
    /// Already inside the archive (a link is enough; it follows the drive).
    pub inside_archive: bool,
}

/// Lets the user choose a file to add; the path stays on the Rust side.
#[tauri::command]
pub async fn pick_addition_file(window: WebviewWindow, state: State<'_, AppState>) -> CmdResult<Option<PickedFile>> {
    let path = match e2e_pick() {
        Some(p) => p,
        None => window
            .app_handle()
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(crate::lang::tr("Add a file to the game (fix, patch, mod, bonus material...)"))
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok()),
    };
    let Some(path) = path else { return Ok(None) };
    register_pick(&state, &path).map(Some).ok_or_else(|| "Please choose a file.".to_string())
}

/// Remembers a chosen or dropped file under a one-time token (None for
/// folders and unreadable entries).
pub fn register_pick(state: &AppState, path: &Path) -> Option<PickedFile> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    let inside = paths::relative_to(&state.layout.root, path)
        .map(|r| paths::is_safe_relative(&paths::to_slash(&r)))
        .unwrap_or(false);
    let n = state.pick_counter.fetch_add(1, Ordering::Relaxed);
    let token = format!("pick-{n}-{}", meta.len());
    let mut picked = lock(&state.picked);
    if picked.len() > 1000 {
        picked.clear(); // tokens are short-lived; never let them pile up
    }
    picked.insert(token.clone(), path.to_path_buf());
    Some(PickedFile {
        token,
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        size: meta.len(),
        location: paths::display(path),
        inside_archive: inside,
    })
}

/// Adds the chosen file: `mode` "copy" (into the archive) or "link" (where it is).
#[tauri::command]
pub async fn add_addition(
    state: State<'_, AppState>,
    game_id: i64,
    token: String,
    mode: String,
    input: AdditionInput,
) -> CmdResult<Addition> {
    let path = lock(&state.picked).remove(&token).ok_or_else(|| "Please choose the file again.".to_string())?;
    match mode.as_str() {
        "link" => state.with_db(|c, l| additions::add_link(c, l, game_id, &path, &input)),
        "copy" => {
            state.with_db(|c, _| additions::check(c, game_id, &input))?;
            if !state.layout.is_available() {
                return Err(CoreError::ArchiveUnavailable.to_string());
            }
            // copying a big file (a documentary) takes a while: no database lock meanwhile
            let layout = state.layout.clone();
            let copied = tauri::async_runtime::spawn_blocking(move || additions::copy_file(&layout, game_id, &path))
                .await
                .map_err(|e| e.to_string())?
                .map_err(err)?;
            state.with_db(|c, l| additions::record_copy(c, l, game_id, &copied, &input))
        }
        _ => Err("Unknown way of adding the file.".to_string()),
    }
}

#[tauri::command]
pub async fn list_additions(state: State<'_, AppState>, game_id: i64) -> CmdResult<Vec<Addition>> {
    state.with_db(|c, l| additions::list(c, l, game_id))
}

#[tauri::command]
pub async fn update_addition(state: State<'_, AppState>, id: i64, input: AdditionInput) -> CmdResult<Addition> {
    state.with_db(|c, l| additions::update(c, l, id, &input))
}

/// Takes a file off the list (the UI asks first). Linked files are never
/// touched; a copy in the archive is deleted only with `deleteCopy`.
#[tauri::command]
pub async fn remove_addition(state: State<'_, AppState>, id: i64, delete_copy: bool) -> CmdResult<()> {
    state.with_db(|c, l| additions::remove(c, l, id, delete_copy).map(|_| ()))
}

#[tauri::command]
pub async fn open_addition(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<&'static str> {
    let p = state.with_db(|c, l| additions::file(c, l, id))?;
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if files::is_risky(&name) {
        return Err(if files::is_runnable(&name) {
            "This is a program. Use \u{201c}Run\u{201d} to start it after confirmation.".to_string()
        } else {
            "For safety, Game Curator does not open script or shortcut files. Use \u{201c}Show in folder\u{201d} instead.".to_string()
        });
    }
    if lock(&state.config).builtin_viewer {
        if let Some(opened) = crate::viewer::open_addition_file(&app, id, &p)? {
            return Ok(opened);
        }
    }
    system::shell_open(&p)?;
    Ok("external")
}

/// Starts an `.exe` / `.msi` additional file (a patch installer). The UI asks first.
#[tauri::command]
pub async fn run_addition(state: State<'_, AppState>, id: i64, confirmed: bool) -> CmdResult<()> {
    if !confirmed {
        return Err("Running a program requires your confirmation.".to_string());
    }
    let p = state.with_db(|c, l| additions::file(c, l, id))?;
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !files::is_runnable(&name) {
        return Err("Only .exe and .msi files can be run.".to_string());
    }
    system::run_program(&p)
}

/// Opens the folder that holds the file.
#[tauri::command]
pub async fn show_addition(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let p = state.with_db(|c, l| additions::file(c, l, id))?;
    let dir = p.parent().ok_or_else(|| "The folder was not found.".to_string())?;
    system::shell_open(dir)
}

// ---------------------------------------------------------------- scanner

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ScanFinished {
    result: Option<ScanResult>,
    error: Option<String>,
}

/// Starts a scan in a background thread. Progress is reported with the
/// `scan-progress` event, the result with `scan-finished`.
#[tauri::command]
pub async fn start_scan(app: AppHandle, state: State<'_, AppState>, root: String, depth: Option<u32>) -> CmdResult<()> {
    let root = root.trim().to_string();
    if root.is_empty() {
        return Err("Please choose the folder to scan.".to_string());
    }
    let cancel = {
        let mut slot = lock(&state.scan_cancel);
        if slot.is_some() {
            return Err("A scan is already running.".to_string());
        }
        let c = Arc::new(AtomicBool::new(false));
        *slot = Some(c.clone());
        c
    };
    let scan_root = paths::resolve(&state.layout.root, &root);
    let archive_root = state.layout.root.clone();
    let depth = depth.unwrap_or(scanner::DEFAULT_MAX_DEPTH as u32) as usize;
    std::thread::spawn(move || {
        let emitter = app.clone();
        let result = scanner::scan(&scan_root, &archive_root, depth, &cancel, &mut |p| {
            let _ = emitter.emit("scan-progress", p);
        });
        let st = app.state::<AppState>();
        let payload = match result {
            Ok(mut r) => match st.with_db(|c, _| scanner::annotate(c, &mut r.candidates)) {
                Ok(()) => ScanFinished { result: Some(r), error: None },
                Err(e) => ScanFinished { result: None, error: Some(e) },
            },
            Err(e) => ScanFinished { result: None, error: Some(e.to_string()) },
        };
        *lock(&st.scan_cancel) = None;
        let _ = app.emit("scan-finished", payload);
    });
    Ok(())
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>) -> CmdResult<bool> {
    match lock(&state.scan_cancel).as_ref() {
        Some(c) => {
            c.store(true, Ordering::Relaxed);
            Ok(true)
        }
        None => Ok(false),
    }
}

#[tauri::command]
pub async fn import_candidates(state: State<'_, AppState>, items: Vec<ImportItem>) -> CmdResult<ImportReport> {
    if items.is_empty() {
        return Err("Select at least one game to import.".to_string());
    }
    state.with_db(|c, l| scanner::import(c, l, &items))
}

// ---------------------------------------------------------------- backups

#[tauri::command]
pub async fn create_backup(state: State<'_, AppState>) -> CmdResult<BackupInfo> {
    state.with_db(|c, l| backup::create_backup(c, l, backup::BackupKind::Manual))
}

#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>) -> CmdResult<Vec<BackupInfo>> {
    backup::list_backups(&state.layout).map_err(err)
}

#[tauri::command]
pub async fn restore_backup(state: State<'_, AppState>, file_name: String) -> CmdResult<RestoreReport> {
    if lock(&state.scan_cancel).is_some() {
        return Err("Please wait until the scan has finished.".to_string());
    }
    let mut db = lock(&state.db);
    let report = backup::restore(&mut db, &state.layout, &file_name).map_err(err)?;
    // The restored database may carry its own archive id.
    if let Ok(conn) = db.conn() {
        if let Ok(Some(id)) = archive::archive_id_of(conn) {
            let file_id = archive::read_id_file(&state.layout.id_file());
            *lock(&state.id_status) = match file_id {
                Some(f) if f != id => ArchiveIdStatus::Mismatch { db_id: id.clone(), file_id: f },
                _ => ArchiveIdStatus::Ok,
            };
            *lock(&state.archive_id) = id;
        }
    }
    Ok(report)
}

#[tauri::command]
pub async fn check_writable(state: State<'_, AppState>) -> CmdResult<()> {
    state.layout.check_writable().map_err(err)
}
