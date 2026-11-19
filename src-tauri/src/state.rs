//! Shared application state.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use curator_core::archive::{ArchiveIdStatus, ArchiveLayout};
use curator_core::config::AppConfig;
use curator_core::db::Database;
use curator_core::rusqlite::Connection;
use curator_core::CoreError;

pub struct AppState {
    pub layout: ArchiveLayout,
    pub db: Mutex<Database>,
    pub config: Mutex<AppConfig>,
    pub archive_id: Mutex<String>,
    pub id_status: Mutex<ArchiveIdStatus>,
    pub db_created: bool,
    pub webview_version: String,
    pub startup_notes: Vec<String>,
    pub scan_cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// Name of this computer (installations are stored per computer).
    pub machine: String,
    /// Files the user chose in a dialog, waiting for "Add" (token -> path),
    /// so the UI never sends a file path of its own.
    pub picked: Mutex<std::collections::HashMap<String, std::path::PathBuf>>,
    pub pick_counter: std::sync::atomic::AtomicU64,
}

/// Locks a mutex even if a previous holder panicked (the data is still valid
/// because every database change is transactional).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn err(e: CoreError) -> String {
    e.to_string()
}

impl AppState {
    /// Runs `f` with the database connection. Storage failures (drive
    /// unplugged) drop the connection so the next call reopens it.
    pub fn with_db<T>(&self, f: impl FnOnce(&mut Connection, &ArchiveLayout) -> Result<T, CoreError>) -> Result<T, String> {
        let mut db = lock(&self.db);
        let result = match db.conn() {
            Ok(conn) => f(conn, &self.layout),
            Err(e) => Err(e),
        };
        result.map_err(|e| {
            if e.is_storage_failure() {
                db.invalidate();
                if !self.layout.is_available() {
                    return CoreError::ArchiveUnavailable.to_string();
                }
            }
            e.to_string()
        })
    }
}
