//! Screenshots: pictures the player took in the game and added by hand.
//!
//! A screenshot belongs to a game and may belong to one of its journal
//! entries. The picture is stored as an optimized copy inside the archive;
//! the player's original file is only read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use crate::archive::ArchiveLayout;
use crate::{db, images, paths, text, CoreError, Result};

const MAX_CAPTION: usize = 500;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub id: i64,
    pub game_id: i64,
    pub journal_entry_id: Option<i64>,
    /// Title of the journal entry it belongs to (for "from the entry ...").
    pub entry_title: Option<String>,
    pub entry_date: Option<String>,
    pub image_path: String,
    pub thumbnail_path: String,
    pub caption: String,
    pub original_name: String,
    pub width: i64,
    pub height: i64,
    pub taken_at: Option<String>,
    pub date_added: String,
}

/// What a journal entry shows of its screenshots.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotRef {
    pub id: i64,
    pub image_path: String,
    pub thumbnail_path: String,
    pub caption: String,
}

const SELECT: &str = "SELECT s.id, s.game_id, s.journal_entry_id, j.title, j.entry_date, s.image_path, s.thumbnail_path,
                             s.caption, s.original_name, s.width, s.height, s.taken_at, s.date_added
                      FROM screenshots s LEFT JOIN journal_entries j ON j.id = s.journal_entry_id";

fn from_row(r: &Row) -> rusqlite::Result<Screenshot> {
    Ok(Screenshot {
        id: r.get(0)?,
        game_id: r.get(1)?,
        journal_entry_id: r.get(2)?,
        entry_title: r.get(3)?,
        entry_date: r.get(4)?,
        image_path: r.get(5)?,
        thumbnail_path: r.get(6)?,
        caption: r.get(7)?,
        original_name: r.get(8)?,
        width: r.get(9)?,
        height: r.get(10)?,
        taken_at: r.get(11)?,
        date_added: r.get(12)?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<Screenshot> {
    conn.query_row(&format!("{SELECT} WHERE s.id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| CoreError::not_found("The screenshot no longer exists."))
}

/// All screenshots of a game, newest first.
pub fn list_for_game(conn: &Connection, game_id: i64) -> Result<Vec<Screenshot>> {
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE s.game_id = ?1 ORDER BY s.date_added DESC, s.id DESC"))?;
    let rows = stmt.query_map([game_id], from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Screenshots of the given journal entries (oldest first within an entry).
pub fn refs_for_entries(conn: &Connection, entry_ids: &[i64]) -> Result<HashMap<i64, Vec<ShotRef>>> {
    let mut out: HashMap<i64, Vec<ShotRef>> = HashMap::new();
    if entry_ids.is_empty() {
        return Ok(out);
    }
    let mut stmt = conn.prepare(
        "SELECT journal_entry_id, id, image_path, thumbnail_path, caption FROM screenshots
         WHERE journal_entry_id = ?1 ORDER BY date_added, id",
    )?;
    for id in entry_ids {
        let rows = stmt.query_map([id], |r| {
            Ok((r.get::<_, i64>(0)?, ShotRef { id: r.get(1)?, image_path: r.get(2)?, thumbnail_path: r.get(3)?, caption: r.get(4)? }))
        })?;
        for row in rows {
            let (entry, shot) = row?;
            out.entry(entry).or_default().push(shot);
        }
    }
    Ok(out)
}

fn entry_game(conn: &Connection, entry_id: i64) -> Result<i64> {
    conn.query_row("SELECT game_id FROM journal_entries WHERE id = ?1", [entry_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| CoreError::not_found("The journal entry no longer exists."))
}

/// Adds a screenshot from a picture file (PNG, JPG or WebP).
pub fn add(conn: &Connection, layout: &ArchiveLayout, game_id: i64, entry_id: Option<i64>, source: &Path) -> Result<Screenshot> {
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM games WHERE id = ?1)", [game_id], |r| r.get(0))?;
    if !exists {
        return Err(CoreError::not_found("The game no longer exists."));
    }
    if let Some(e) = entry_id {
        if entry_game(conn, e)? != game_id {
            return Err(CoreError::invalid("The journal entry belongs to another game."));
        }
    }
    let shot = images::process_screenshot(layout, game_id, source)?;
    let taken = std::fs::metadata(source)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let now = db::now_iso(conn)?;
    let inserted = conn.execute(
        "INSERT INTO screenshots (game_id, journal_entry_id, image_path, thumbnail_path, original_name, width, height, taken_at, date_added)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                 CASE WHEN ?8 IS NULL THEN NULL ELSE strftime('%Y-%m-%dT%H:%M:%S', ?8, 'unixepoch', 'localtime') END, ?9)",
        params![game_id, entry_id, shot.image_path, shot.thumbnail_path, name, shot.width, shot.height, taken, now],
    );
    if let Err(e) = inserted {
        images::remove_screenshot_files(layout, &shot.image_path, &shot.thumbnail_path);
        return Err(e.into());
    }
    get(conn, conn.last_insert_rowid())
}

pub fn set_caption(conn: &Connection, id: i64, caption: &str) -> Result<Screenshot> {
    let caption = text::tidy(caption);
    if caption.chars().count() > MAX_CAPTION {
        return Err(CoreError::invalid(format!("The caption is too long (maximum {MAX_CAPTION} characters).")));
    }
    let n = conn.execute("UPDATE screenshots SET caption = ?1 WHERE id = ?2", params![caption, id])?;
    if n == 0 {
        return Err(CoreError::not_found("The screenshot no longer exists."));
    }
    get(conn, id)
}

/// Attaches screenshots to a journal entry of the same game, or detaches them
/// (`entry_id` = None; they stay in the game's gallery). Returns the journal
/// entries whose screenshots changed (to refresh their HTML copies).
pub fn attach(conn: &mut Connection, ids: &[i64], entry_id: Option<i64>) -> Result<Vec<i64>> {
    let target_game = entry_id.map(|e| entry_game(conn, e)).transpose()?;
    let tx = conn.transaction()?;
    let mut touched = Vec::new();
    for id in ids {
        let (game, prev): (i64, Option<i64>) = tx
            .query_row("SELECT game_id, journal_entry_id FROM screenshots WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?
            .ok_or_else(|| CoreError::not_found("The screenshot no longer exists."))?;
        if let Some(g) = target_game {
            if g != game {
                return Err(CoreError::invalid("A screenshot can only be attached to a journal entry of the same game."));
            }
        }
        tx.execute("UPDATE screenshots SET journal_entry_id = ?1 WHERE id = ?2", params![entry_id, id])?;
        touched.extend(prev);
    }
    tx.commit()?;
    touched.extend(entry_id);
    touched.sort_unstable();
    touched.dedup();
    Ok(touched)
}

/// When a journal entry moves to another game, its screenshots go with it.
pub fn follow_entry(conn: &Connection, entry_id: i64, game_id: i64) -> Result<()> {
    conn.execute("UPDATE screenshots SET game_id = ?1 WHERE journal_entry_id = ?2", params![game_id, entry_id])?;
    Ok(())
}

/// Deletes a screenshot (after the user confirmed it): the record and the
/// stored copy. The player's original file is not touched.
pub fn delete(conn: &Connection, layout: &ArchiveLayout, id: i64) -> Result<Screenshot> {
    let shot = get(conn, id)?;
    conn.execute("DELETE FROM screenshots WHERE id = ?1", [id])?;
    images::remove_screenshot_files(layout, &shot.image_path, &shot.thumbnail_path);
    Ok(shot)
}

/// Files of all screenshots of a game (removed together with the game).
pub fn files_of_game(conn: &Connection, game_id: i64) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT image_path, thumbnail_path FROM screenshots WHERE game_id = ?1")?;
    let rows = stmt.query_map([game_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The stored picture, for opening it in the Windows photo viewer.
pub fn image_file(conn: &Connection, layout: &ArchiveLayout, id: i64) -> Result<PathBuf> {
    let shot = get(conn, id)?;
    if !paths::is_safe_relative(&shot.image_path) || !shot.image_path.starts_with("Data/Screenshots/") {
        return Err(CoreError::PathOutsideAllowed);
    }
    let p = layout.root.join(&shot.image_path);
    if !p.is_file() {
        return Err(CoreError::not_found("The screenshot file is missing."));
    }
    Ok(p)
}
