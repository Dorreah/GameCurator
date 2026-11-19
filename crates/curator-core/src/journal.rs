//! Gaming Journal entries.
//!
//! The database is the source of truth. In addition, every entry is mirrored
//! as a self-contained HTML file in `Data/Journals/game-<id>/entry-<id>.html`
//! so the journal stays readable even without Game Curator (long-term
//! archiving). Mirror failures never block saving.

use std::fs;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::archive::ArchiveLayout;
use crate::screenshots::{self, ShotRef};
use crate::{db, text, CoreError, Result};

const MAX_CONTENT: usize = 1_000_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub id: i64,
    pub game_id: i64,
    pub game_title: String,
    pub entry_date: String,
    pub title: String,
    pub content: String,
    pub date_created: String,
    pub date_modified: String,
    /// Screenshots attached to the entry.
    pub screenshots: Vec<ShotRef>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct JournalInput {
    pub game_id: i64,
    pub entry_date: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct JournalQuery {
    pub game_id: Option<i64>,
    /// "desc" (newest first, default) or "asc"
    pub order: Option<String>,
    pub limit: Option<i64>,
}

pub fn validate_date(d: &str) -> Result<String> {
    let d = d.trim();
    let b = d.as_bytes();
    let ok = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    if ok {
        let y: u32 = d[0..4].parse().unwrap_or(0);
        let m: u32 = d[5..7].parse().unwrap_or(0);
        let day: u32 = d[8..10].parse().unwrap_or(0);
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let dim = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => 0,
        };
        if (1900..=2200).contains(&y) && day >= 1 && day <= dim {
            return Ok(d.to_string());
        }
    }
    Err(CoreError::invalid("Please enter a valid date (YYYY-MM-DD)."))
}

fn validate(conn: &Connection, input: &JournalInput) -> Result<(String, String, String)> {
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM games WHERE id = ?1)", [input.game_id], |r| r.get(0))?;
    if !exists {
        return Err(CoreError::invalid("Please choose the game this entry belongs to."));
    }
    let date = validate_date(&input.entry_date)?;
    let title = input.title.trim().to_string();
    if title.chars().count() > 200 {
        return Err(CoreError::invalid("Entry title is too long (maximum 200 characters)."));
    }
    if input.content.len() > MAX_CONTENT {
        return Err(CoreError::invalid("The entry is too long."));
    }
    if title.is_empty() && plain_text(&input.content).trim().is_empty() {
        return Err(CoreError::invalid("The entry is empty. Write a title or some text."));
    }
    Ok((date, title, input.content.clone()))
}

const SELECT: &str = "SELECT j.id, j.game_id, g.title, j.entry_date, j.title, j.content, j.date_created, j.date_modified
                      FROM journal_entries j JOIN games g ON g.id = j.game_id";

fn from_row(r: &Row) -> rusqlite::Result<JournalEntry> {
    Ok(JournalEntry {
        id: r.get(0)?,
        game_id: r.get(1)?,
        game_title: r.get(2)?,
        entry_date: r.get(3)?,
        title: r.get(4)?,
        content: r.get(5)?,
        date_created: r.get(6)?,
        date_modified: r.get(7)?,
        screenshots: Vec::new(),
    })
}

fn with_screenshots(conn: &Connection, mut entries: Vec<JournalEntry>) -> Result<Vec<JournalEntry>> {
    let ids: Vec<i64> = entries.iter().map(|e| e.id).collect();
    let mut refs = screenshots::refs_for_entries(conn, &ids)?;
    for e in &mut entries {
        e.screenshots = refs.remove(&e.id).unwrap_or_default();
    }
    Ok(entries)
}

pub fn get_entry(conn: &Connection, id: i64) -> Result<JournalEntry> {
    let e = conn
        .query_row(&format!("{SELECT} WHERE j.id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| CoreError::not_found("The journal entry no longer exists."))?;
    Ok(with_screenshots(conn, vec![e])?.remove(0))
}

pub fn list_entries(conn: &Connection, q: &JournalQuery) -> Result<Vec<JournalEntry>> {
    let order = if q.order.as_deref() == Some("asc") { "ASC" } else { "DESC" };
    let limit = q.limit.filter(|l| *l > 0).unwrap_or(-1);
    let sql = match q.game_id {
        Some(_) => format!("{SELECT} WHERE j.game_id = ?1 ORDER BY j.entry_date {order}, j.id {order} LIMIT ?2"),
        None => format!("{SELECT} WHERE ?1 IS NULL ORDER BY j.entry_date {order}, j.id {order} LIMIT ?2"),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![q.game_id, limit], from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    with_screenshots(conn, rows)
}

pub fn create_entry(conn: &mut Connection, input: &JournalInput) -> Result<JournalEntry> {
    let (date, title, content) = validate(conn, input)?;
    let tx = conn.transaction()?;
    let now = db::now_iso(&tx)?;
    tx.execute(
        "INSERT INTO journal_entries (game_id, entry_date, title, content, date_created, date_modified)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![input.game_id, date, title, content, now],
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    get_entry(conn, id)
}

/// Updates an entry. Returns the new entry and the game id it belonged to
/// before (so the caller can move the HTML mirror).
pub fn update_entry(conn: &mut Connection, id: i64, input: &JournalInput) -> Result<(JournalEntry, i64)> {
    let previous = get_entry(conn, id)?;
    let (date, title, content) = validate(conn, input)?;
    let tx = conn.transaction()?;
    let now = db::now_iso(&tx)?;
    tx.execute(
        "UPDATE journal_entries SET game_id = ?1, entry_date = ?2, title = ?3, content = ?4, date_modified = ?5
         WHERE id = ?6",
        params![input.game_id, date, title, content, now, id],
    )?;
    if previous.game_id != input.game_id {
        screenshots::follow_entry(&tx, id, input.game_id)?;
    }
    tx.commit()?;
    let entry = get_entry(conn, id)?;
    Ok((entry, previous.game_id))
}

pub fn delete_entry(conn: &mut Connection, id: i64) -> Result<JournalEntry> {
    let entry = get_entry(conn, id)?;
    conn.execute("DELETE FROM journal_entries WHERE id = ?1", [id])?;
    Ok(entry)
}

/// Very small tag stripper for previews and validation (content is HTML).
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    text::tidy(&out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">"))
}

// ---------------------------------------------------------------- HTML mirror

fn mirror_path_for(game_id: i64, entry_id: i64) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("game-{game_id}")).join(format!("entry-{entry_id}.html"))
}

pub fn write_mirror(layout: &ArchiveLayout, e: &JournalEntry) -> Result<()> {
    let rel = mirror_path_for(e.game_id, e.id);
    let path = layout.journals_dir().join(&rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let html = format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>{title} - {game}</title>\
         <style>body{{font-family:Georgia,serif;max-width:46em;margin:2em auto;color:#222}}\
         .m{{color:#666;font-size:.9em}}</style></head><body>\
         <p class=\"m\">Game Curator journal &middot; {game} &middot; {date}</p><h1>{title}</h1>\
         <article>{content}</article>{shots}<p class=\"m\">Created {created}, modified {modified}</p></body></html>\n",
        title = text::escape_html(if e.title.is_empty() { "(untitled)" } else { &e.title }),
        game = text::escape_html(&e.game_title),
        date = text::escape_html(&e.entry_date),
        content = e.content,
        shots = mirror_shots(&e.screenshots),
        created = text::escape_html(&e.date_created),
        modified = text::escape_html(&e.date_modified),
    );
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, html)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Screenshots in the HTML copy, linked relative to `Data/Journals/game-<id>/`.
fn mirror_shots(shots: &[ShotRef]) -> String {
    let figs: String = shots
        .iter()
        .filter_map(|s| s.image_path.strip_prefix("Data/").map(|rest| (s, rest)))
        .map(|(s, rest)| {
            let cap = if s.caption.is_empty() { String::new() } else { format!("<figcaption>{}</figcaption>", text::escape_html(&s.caption)) };
            format!("<figure><a href=\"../../{rest}\"><img src=\"../../{rest}\" alt=\"\" style=\"max-width:100%\"></a>{cap}</figure>", rest = text::escape_html(rest))
        })
        .collect();
    if figs.is_empty() {
        String::new()
    } else {
        format!("<section class=\"shots\">{figs}</section>")
    }
}

pub fn remove_mirror(layout: &ArchiveLayout, game_id: i64, entry_id: i64) {
    let _ = fs::remove_file(layout.journals_dir().join(mirror_path_for(game_id, entry_id)));
}

pub fn remove_game_mirrors(layout: &ArchiveLayout, game_id: i64) {
    let _ = fs::remove_dir_all(layout.journals_dir().join(format!("game-{game_id}")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert!(validate_date("2026-11-18").is_ok());
        assert!(validate_date("2024-02-29").is_ok());
        assert!(validate_date("2023-02-29").is_err());
        assert!(validate_date("2026-13-01").is_err());
        assert!(validate_date("18.11.2026").is_err());
    }

    #[test]
    fn strip_tags() {
        assert_eq!(plain_text("<h2>Day&nbsp;1</h2><p>Hello <b>world</b></p>"), "Day 1 Hello world");
    }
}
