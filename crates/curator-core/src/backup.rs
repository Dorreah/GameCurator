//! Database backups (`Data/Backups/`) and restore.
//!
//! Backups are made with `VACUUM INTO`, which writes a consistent, compact
//! copy even while the database is in use. Every backup is verified after it
//! has been written. Restoring first makes a safety backup of the current
//! database, then swaps the files and reopens the connection.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use crate::archive::{self, ArchiveLayout};
use crate::db::{self, Database};
use crate::{CoreError, Result};

pub const AUTO_BACKUP_INTERVAL_HOURS: i64 = 24;
const SETTING_LAST_AUTO: &str = "last_auto_backup";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackupKind {
    Manual,
    Auto,
    PreRestore,
}

impl BackupKind {
    fn suffix(self) -> &'static str {
        match self {
            BackupKind::Manual => "manual",
            BackupKind::Auto => "auto",
            BackupKind::PreRestore => "pre-restore",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub file_name: String,
    pub size_bytes: u64,
    /// "manual" | "auto" | "pre-restore"
    pub kind: String,
    /// Taken from the file name: YYYY-MM-DD HH:MM:SS
    pub created: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupCheck {
    pub games: i64,
    pub journal_entries: i64,
    pub archive_id: Option<String>,
}

fn parse_name(name: &str) -> Option<(String, String)> {
    // library-20261118-184512-manual.db
    let stem = name.strip_prefix("library-")?.strip_suffix(".db")?;
    let mut parts = stem.splitn(3, '-');
    let date = parts.next()?;
    let time = parts.next()?;
    let kind = parts.next().unwrap_or("manual");
    if date.len() != 8 || time.len() != 6 || !date.chars().chain(time.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let created = format!(
        "{}-{}-{} {}:{}:{}",
        &date[0..4], &date[4..6], &date[6..8], &time[0..2], &time[2..4], &time[4..6]
    );
    // strip "-2" style uniqueness counters
    let kind = kind.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_').to_string();
    Some((created, kind))
}

/// Verifies that a file is a healthy Game Curator database.
pub fn verify(path: &Path) -> Result<BackupCheck> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|_| CoreError::invalid("The backup file cannot be opened."))?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|_| CoreError::invalid("The backup file is not a valid database."))?;
    if check != "ok" {
        return Err(CoreError::invalid(format!("The backup file is damaged ({check}).")));
    }
    let has_games: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'games')",
        [],
        |r| r.get(0),
    )?;
    if !has_games {
        return Err(CoreError::invalid("The file is not a Game Curator database."));
    }
    Ok(BackupCheck {
        games: conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?,
        journal_entries: conn.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r.get(0)).unwrap_or(0),
        archive_id: archive::archive_id_of(&conn).ok().flatten(),
    })
}

pub fn create_backup(conn: &Connection, layout: &ArchiveLayout, kind: BackupKind) -> Result<BackupInfo> {
    let dir = layout.backups_dir();
    fs::create_dir_all(&dir)?;
    let stamp = db::now(conn, "%Y%m%d-%H%M%S")?;
    let mut name = format!("library-{stamp}-{}.db", kind.suffix());
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("library-{stamp}-{}{n}.db", kind.suffix());
        n += 1;
    }
    let path = dir.join(&name);
    let tmp = dir.join(format!("{name}.tmp"));
    let _ = fs::remove_file(&tmp);
    conn.execute("VACUUM INTO ?1", [tmp.to_string_lossy().as_ref()])?;
    if let Err(e) = verify(&tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, &path)?;
    let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let (created, kind_s) = parse_name(&name).unwrap_or_default();
    Ok(BackupInfo { file_name: name, size_bytes: size, kind: kind_s, created })
}

pub fn list_backups(layout: &ArchiveLayout) -> Result<Vec<BackupInfo>> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(layout.backups_dir()) else { return Ok(out) };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some((created, kind)) = parse_name(&name) {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(BackupInfo { file_name: name, size_bytes: size, kind, created });
        }
    }
    out.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.file_name.cmp(&a.file_name)));
    Ok(out)
}

/// Keeps the newest `keep` automatic backups and deletes older automatic
/// ones. Manual and pre-restore backups are never deleted automatically.
pub fn prune_auto_backups(layout: &ArchiveLayout, keep: usize) -> Result<usize> {
    let autos: Vec<BackupInfo> = list_backups(layout)?.into_iter().filter(|b| b.kind == "auto").collect();
    let mut removed = 0;
    for b in autos.into_iter().skip(keep.max(1)) {
        if fs::remove_file(layout.backups_dir().join(&b.file_name)).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Creates an automatic backup when the last one is older than the interval.
/// Returns the backup when one was made.
pub fn auto_backup_if_due(conn: &Connection, layout: &ArchiveLayout, keep: usize) -> Result<Option<BackupInfo>> {
    let games: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?;
    if games == 0 {
        return Ok(None); // nothing worth backing up yet
    }
    let due: bool = match db::get_setting(conn, SETTING_LAST_AUTO)? {
        None => true,
        Some(last) => conn.query_row(
            "SELECT julianday('now', 'localtime') - julianday(?1) >= ?2 / 24.0",
            rusqlite::params![last, AUTO_BACKUP_INTERVAL_HOURS],
            |r| r.get(0),
        )?,
    };
    if !due {
        return Ok(None);
    }
    let info = create_backup(conn, layout, BackupKind::Auto)?;
    db::set_setting(conn, SETTING_LAST_AUTO, &db::now_iso(conn)?)?;
    prune_auto_backups(layout, keep)?;
    Ok(Some(info))
}

fn backup_path(layout: &ArchiveLayout, file_name: &str) -> Result<PathBuf> {
    if file_name.contains(['/', '\\']) || file_name.contains("..") || parse_name(file_name).is_none() {
        return Err(CoreError::PathOutsideAllowed);
    }
    let p = layout.backups_dir().join(file_name);
    if !p.is_file() {
        return Err(CoreError::not_found("The backup file no longer exists."));
    }
    Ok(p)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub restored_from: String,
    pub safety_backup: String,
    pub games: i64,
    pub journal_entries: i64,
}

/// Replaces the current database with a backup from `Data/Backups/`.
pub fn restore(db: &mut Database, layout: &ArchiveLayout, file_name: &str) -> Result<RestoreReport> {
    let src = backup_path(layout, file_name)?;
    let check = verify(&src)?;
    let safety = create_backup(db.conn()?, layout, BackupKind::PreRestore)?;

    let db_path = layout.db_path();
    let staged = db_path.with_extension("db.restoring");
    fs::copy(&src, &staged)?;
    verify(&staged)?;

    db.close();
    let swap = fs::rename(&staged, &db_path);
    // Always reopen - on failure the old database is still in place.
    let reopened = Database::open(&db_path);
    match (swap, reopened) {
        (Ok(()), Ok(new_db)) => {
            *db = new_db;
            Ok(RestoreReport {
                restored_from: file_name.to_string(),
                safety_backup: safety.file_name,
                games: check.games,
                journal_entries: check.journal_entries,
            })
        }
        (Err(e), reopened) => {
            if let Ok(d) = reopened {
                *db = d;
            }
            let _ = fs::remove_file(&staged);
            Err(CoreError::invalid(format!("Restore failed, the current database was kept: {e}")))
        }
        (Ok(()), Err(e)) => {
            // The restored file cannot be opened: put the safety copy back.
            let _ = fs::copy(layout.backups_dir().join(&safety.file_name), &db_path);
            if let Ok(d) = Database::open(&db_path) {
                *db = d;
            }
            Err(CoreError::invalid(format!("Restore failed, the previous database was put back: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(
            parse_name("library-20261118-184512-manual.db"),
            Some(("2026-11-18 18:45:12".into(), "manual".into()))
        );
        assert_eq!(parse_name("library-20261118-184512-auto2.db").unwrap().1, "auto");
        assert_eq!(parse_name("library-20261118-184512-pre-restore.db").unwrap().1, "pre-restore");
        assert!(parse_name("evil.db").is_none());
        assert!(parse_name("library-2026-x.db").is_none());
    }
}
