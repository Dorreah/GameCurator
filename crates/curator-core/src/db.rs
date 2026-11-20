//! SQLite connection management, migrations and key/value settings.
//!
//! Durability choices for removable drives:
//!  * `journal_mode = DELETE` - committed data always lives in `library.db`
//!    itself; there is no `-wal` side file that could be forgotten when the
//!    archive is copied.
//!  * `synchronous = FULL` - every commit is flushed before it is reported as
//!    done, so pulling the USB drive cannot corrupt the database.
//!  * every multi-step change runs inside a transaction.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::{CoreError, Result};

pub const SCHEMA_VERSION: i64 = 5;
/// Current schema, used for new databases.
const SCHEMA: &str = include_str!("schema.sql");
/// Version 1 -> 2: play status, ratings and favourites were removed.
const MIGRATE_1_TO_2: &str = "
    DROP INDEX IF EXISTS idx_games_status;
    DROP INDEX IF EXISTS idx_games_favorite;
    ALTER TABLE games DROP COLUMN status;
    ALTER TABLE games DROP COLUMN rating;
    ALTER TABLE games DROP COLUMN is_favorite;
    ALTER TABLE journal_entries DROP COLUMN rating;
    PRAGMA user_version = 2;";
/// Version 2 -> 3: chosen crop of the cover artwork, installed games per PC.
const MIGRATE_2_TO_3: &str = "
    ALTER TABLE games ADD COLUMN cover_crop TEXT;
    CREATE TABLE IF NOT EXISTS installations (
        game_id      INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
        machine      TEXT    NOT NULL,
        install_dir  TEXT    NOT NULL,
        exe_path     TEXT    NOT NULL DEFAULT '',
        arguments    TEXT    NOT NULL DEFAULT '',
        working_dir  TEXT    NOT NULL DEFAULT '',
        source       TEXT    NOT NULL CHECK (source IN ('gog', 'uninstall', 'manual')),
        date_updated TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
        PRIMARY KEY (game_id, machine)
    ) WITHOUT ROWID;
    PRAGMA user_version = 3;";
/// Version 3 -> 4: screenshots (optionally attached to a journal entry) and
/// additional files (fixes, patches, bonus material).
const MIGRATE_3_TO_4: &str = "
    CREATE TABLE IF NOT EXISTS screenshots (
        id               INTEGER PRIMARY KEY,
        game_id          INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
        journal_entry_id INTEGER REFERENCES journal_entries (id) ON DELETE SET NULL,
        image_path       TEXT    NOT NULL,
        thumbnail_path   TEXT    NOT NULL,
        caption          TEXT    NOT NULL DEFAULT '',
        original_name    TEXT    NOT NULL DEFAULT '',
        width            INTEGER NOT NULL DEFAULT 0,
        height           INTEGER NOT NULL DEFAULT 0,
        taken_at         TEXT,
        date_added       TEXT    NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_screenshots_game ON screenshots (game_id, date_added DESC);
    CREATE INDEX IF NOT EXISTS idx_screenshots_entry ON screenshots (journal_entry_id);
    CREATE TABLE IF NOT EXISTS additions (
        id          INTEGER PRIMARY KEY,
        game_id     INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
        kind        TEXT    NOT NULL CHECK (kind IN ('copy', 'link')),
        path        TEXT    NOT NULL,
        filename    TEXT    NOT NULL,
        category    TEXT    NOT NULL DEFAULT 'other' CHECK (category IN ('fix', 'patch', 'mod', 'bonus', 'other')),
        note        TEXT    NOT NULL DEFAULT '',
        size_bytes  INTEGER NOT NULL DEFAULT 0,
        date_added  TEXT    NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_additions_game ON additions (game_id);
    PRAGMA user_version = 4;";
/// Version 4 -> 5: icons of installed games (a cached copy) and names shown in
/// the catalog for archive files and additional files (the files on disk keep
/// their names).
const MIGRATE_4_TO_5: &str = "
    ALTER TABLE games ADD COLUMN icon_path TEXT;
    ALTER TABLE files ADD COLUMN display_name TEXT;
    ALTER TABLE additions ADD COLUMN display_name TEXT;
    PRAGMA user_version = 5;";

pub fn open_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = DELETE;
         PRAGMA synchronous = FULL;
         PRAGMA temp_store = MEMORY;",
    )?;
    Ok(conn)
}

/// Brings the schema up to date. Before an existing database is changed,
/// a copy is written to `Backups/library-<time>-pre-upgrade.db` next to it
/// (when `db_path` is known), so an upgrade can never lose data.
pub fn migrate(conn: &Connection, db_path: Option<&Path>) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(CoreError::invalid(
            "This database was created by a newer version of Game Curator. Please use the newer version.",
        ));
    }
    if version == 0 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA)?;
        tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        tx.commit()?;
        return Ok(());
    }
    if version < SCHEMA_VERSION {
        if let Some(path) = db_path {
            pre_upgrade_backup(conn, path)?;
        }
        let tx = conn.unchecked_transaction()?;
        if version < 2 {
            tx.execute_batch(MIGRATE_1_TO_2)?;
        }
        if version < 3 {
            tx.execute_batch(MIGRATE_2_TO_3)?;
        }
        if version < 4 {
            tx.execute_batch(MIGRATE_3_TO_4)?;
        }
        if version < 5 {
            tx.execute_batch(MIGRATE_4_TO_5)?;
        }
        tx.commit()?;
    }
    Ok(())
}

fn pre_upgrade_backup(conn: &Connection, db_path: &Path) -> Result<()> {
    let dir = db_path.parent().map(|p| p.join("Backups")).unwrap_or_else(|| PathBuf::from("Backups"));
    std::fs::create_dir_all(&dir)?;
    let stamp = now(conn, "%Y%m%d-%H%M%S")?;
    let mut target = dir.join(format!("library-{stamp}-pre-upgrade.db"));
    let mut n = 2;
    while target.exists() {
        target = dir.join(format!("library-{stamp}-pre-upgrade{n}.db"));
        n += 1;
    }
    conn.execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])?;
    Ok(())
}

/// Owns the single SQLite connection of the application.
pub struct Database {
    path: PathBuf,
    conn: Option<Connection>,
}

impl Database {
    /// Opens (or creates) the database and brings the schema up to date.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = open_connection(path)?;
        migrate(&conn, Some(path))?;
        Ok(Database { path: path.to_path_buf(), conn: Some(conn) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the connection after verifying the database file is still
    /// reachable. Reopens the connection if it was invalidated earlier.
    pub fn conn(&mut self) -> Result<&mut Connection> {
        if !self.path.is_file() {
            return Err(CoreError::ArchiveUnavailable);
        }
        if self.conn.is_none() {
            let c = open_connection(&self.path)?;
            migrate(&c, Some(&self.path))?;
            self.conn = Some(c);
        }
        Ok(self.conn.as_mut().expect("connection just opened"))
    }

    /// Drops the connection (e.g. after an I/O error); the next `conn()` reopens it.
    pub fn invalidate(&mut self) {
        if let Some(c) = self.conn.take() {
            let _ = c.close();
        }
    }

    /// Closes the connection cleanly. Safe to call several times.
    pub fn close(&mut self) {
        if let Some(c) = self.conn.take() {
            let _ = c.execute_batch("PRAGMA optimize;");
            let _ = c.close();
        }
    }

    pub fn is_open(&self) -> bool {
        self.conn.is_some()
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT setting_value FROM settings WHERE setting_key = ?1", [key], |r| r.get::<_, Option<String>>(0))
        .optional()?
        .flatten())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (setting_key, setting_value) VALUES (?1, ?2)
         ON CONFLICT(setting_key) DO UPDATE SET setting_value = excluded.setting_value",
        params![key, value],
    )?;
    Ok(())
}

/// Current local time formatted by SQLite (avoids a date/time dependency).
pub fn now(conn: &Connection, fmt: &str) -> Result<String> {
    Ok(conn.query_row("SELECT strftime(?1, 'now', 'localtime')", [fmt], |r| r.get(0))?)
}

pub fn now_iso(conn: &Connection) -> Result<String> {
    now(conn, "%Y-%m-%dT%H:%M:%S")
}
