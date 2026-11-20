//! Game Curator - Tauri application layer (Windows 10/11, WebView2).
//!
//! Start-up sequence:
//! 1. locate the archive root (folder of the exe),
//! 2. verify that a WebView runtime exists (clear message otherwise),
//! 3. open the archive (folder structure, database, archive id),
//! 4. housekeeping: remove orphaned artwork, automatic backup when due,
//! 5. create the window with its WebView profile stored in `Data/WebView`
//!    (nothing persistent goes to %LOCALAPPDATA%),
//! 6. on exit: stop a running scan and close the database cleanly.

#[cfg(all(not(windows), not(feature = "e2e")))]
compile_error!(
    "Game Curator is a Windows application. Non-Windows builds exist only as the automated test harness: \
     build with `--features e2e`."
);

mod commands;
mod dragdrop;
mod protocol;
mod session;
mod state;
mod system;
mod viewer;
mod offer;

use std::sync::atomic::Ordering;
use std::sync::Mutex;

use curator_core::archive::{self, OpenedArchive};
use curator_core::{backup, config, images, paths};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use state::{lock, AppState};

const TITLE: &str = "Game Curator";

/// WebView2 switches: keep Tauri's defaults (no SmartScreen / Office UI
/// services) and switch off every background network service, sync and most
/// disk caching - the app is offline and its pages come from inside the exe,
/// so this saves memory, CPU and writes to the USB drive. The music player
/// may start the track the user opened without a second click.
#[cfg(windows)]
pub(crate) const WEBVIEW2_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
--disable-background-networking --disable-component-update --disable-domain-reliability --disable-sync \
--no-pings --disk-cache-size=1048576 --disable-gpu-shader-disk-cache --autoplay-policy=no-user-gesture-required";

pub fn run() {
    let root = match system::archive_root() {
        Ok(r) => r,
        Err(e) => {
            system::fatal(TITLE, &e);
            std::process::exit(1);
        }
    };
    system::prepare_webview_runtime(&root);
    let webview_version = match system::check_webview() {
        Ok(v) => v,
        Err(msg) => {
            system::fatal(TITLE, &msg);
            std::process::exit(2);
        }
    };
    let OpenedArchive { layout, mut db, archive_id, id_status, db_created } = match archive::open_archive(&root) {
        Ok(o) => o,
        Err(e) => {
            system::fatal(
                TITLE,
                &format!(
                    "The archive could not be opened.\n\nLocation: {}\n\n{e}\n\nIf the drive is write-protected, remove the protection and start Game Curator again.",
                    paths::display(&root)
                ),
            );
            std::process::exit(3);
        }
    };

    let cfg = config::load(&layout.config_path());
    let mut notes = Vec::new();
    if let Ok(conn) = db.conn() {
        match images::cleanup_orphans(conn, &layout) {
            Ok(n) if n > 0 => notes.push(format!("Removed {n} unused artwork file(s).")),
            _ => {}
        }
        if cfg.auto_backup {
            match backup::auto_backup_if_due(conn, &layout, cfg.auto_backup_keep as usize) {
                Ok(Some(b)) => notes.push(format!("Automatic database backup created ({}).", b.file_name)),
                Ok(None) => {}
                Err(e) => notes.push(format!("Automatic backup failed: {e}")),
            }
        }
    }

    let webview_dir = layout.webview_dir();
    let state = AppState {
        layout,
        db: Mutex::new(db),
        config: Mutex::new(cfg),
        archive_id: Mutex::new(archive_id),
        id_status: Mutex::new(id_status),
        db_created,
        webview_version,
        startup_notes: notes,
        scan_cancel: Mutex::new(None),
        machine: system::machine_name(),
        picked: Mutex::new(Default::default()),
        pick_counter: Default::default(),
        running_games: Mutex::new(Default::default()),
        viewers: Mutex::new(Default::default()),
        offers: Mutex::new(Default::default()),
    };

    let built = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .register_uri_scheme_protocol("gcimg", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::adopt_database,
            commands::archive_status,
            commands::get_config,
            commands::save_config,
            commands::list_games,
            commands::library_facets,
            commands::game_titles,
            commands::get_game,
            commands::create_game,
            commands::update_game,
            commands::patch_game,
            commands::removal_preview,
            commands::remove_game,
            commands::check_availability,
            commands::pick_folder,
            commands::pick_artwork,
            commands::set_cover_crop,
            commands::detect_installs,
            commands::install_status,
            commands::locate_install,
            commands::forget_install,
            commands::open_install_folder,
            commands::play_game,
            commands::add_screenshots,
            commands::add_dropped_screenshots,
            commands::e2e_drop,
            commands::list_screenshots,
            commands::set_screenshot_caption,
            commands::attach_screenshots,
            commands::delete_screenshot,
            commands::open_screenshot,
            commands::pick_addition_file,
            commands::add_addition,
            commands::list_additions,
            commands::update_addition,
            commands::remove_addition,
            commands::open_addition,
            commands::run_addition,
            commands::show_addition,
            commands::archive_content,
            commands::set_file_category,
            commands::set_file_display_name,
            commands::running_games,
            commands::set_primary_file,
            commands::open_file,
            commands::run_file,
            commands::open_game_folder,
            commands::list_journal,
            commands::create_journal,
            commands::update_journal,
            commands::delete_journal,
            commands::start_scan,
            commands::cancel_scan,
            commands::import_candidates,
            commands::create_backup,
            commands::list_backups,
            commands::restore_backup,
            commands::check_writable,
            viewer::viewer_document,
            viewer::player_state,
            viewer::media_read,
            viewer::media_open_external,
            offer::journal_offer,
            offer::answer_journal_offer,
        ])
        .setup(move |app| {
            std::fs::create_dir_all(&webview_dir)?;
            let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Game Curator 0.1")
                .inner_size(1280.0, 820.0)
                .min_inner_size(1000.0, 640.0)
                .data_directory(webview_dir.clone());
            #[cfg(windows)]
            let builder = builder.additional_browser_args(WEBVIEW2_ARGS);
            let win = builder.build()?;
            let drop_win = win.clone();
            let handle = app.handle().clone();
            win.on_window_event(move |ev| match ev {
                tauri::WindowEvent::DragDrop(d) => dragdrop::handle(&drop_win, d),
                // the PDF viewers and the music player close with the main window
                tauri::WindowEvent::Destroyed => viewer::close_all(&handle),
                _ => {}
            });
            Ok(())
        })
        .build(tauri::generate_context!());

    let app = match built {
        Ok(app) => app,
        Err(e) => {
            system::fatal(TITLE, &format!("The application window could not be created.\n\n{e}"));
            std::process::exit(4);
        }
    };

    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            let st = handle.state::<AppState>();
            if let Some(c) = lock(&st.scan_cancel).as_ref() {
                c.store(true, Ordering::Relaxed);
            }
            lock(&st.db).close();
        }
    });
}
