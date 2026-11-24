//! Game catalog: create, read, update, remove, library queries, tags,
//! availability checks.
//!
//! Game Curator is an archive, not a backlog: there is deliberately no
//! play status, score or "completed" tracking - only what helps to keep the
//! installers, manuals and extras of each game in order.
//!
//! Removing a game from the catalog **never** touches its folder on disk.

use std::path::Path;

use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};

use crate::archive::ArchiveLayout;
use crate::images::Crop;
use crate::{db, images, paths, text, CoreError, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCard {
    pub id: i64,
    pub title: String,
    pub developer: String,
    pub genre: String,
    pub release_year: Option<i64>,
    pub is_available: bool,
    pub thumbnail_path: Option<String>,
    /// Folder as stored (relative to the archive when possible).
    pub folder_path: String,
    /// Catalogued files that are present (missing ones are not counted).
    pub file_count: i64,
    pub total_size: i64,
    /// Installed on this computer (detected or located by the user).
    pub is_installed: bool,
    /// Cached icon of the installed game (Data/Thumbnails/icon-*.webp).
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDetail {
    pub id: i64,
    pub title: String,
    /// Path as stored in the database (relative to the archive root when possible).
    pub folder_path: String,
    /// Absolute path on the current computer.
    pub folder_resolved: String,
    pub folder_portable: bool,
    pub folder_exists: bool,
    pub description: String,
    pub developer: String,
    pub publisher: String,
    pub release_year: Option<i64>,
    pub genre: String,
    pub artwork_path: Option<String>,
    pub thumbnail_path: Option<String>,
    /// Part of the cover shown in the frame (None = centred).
    pub cover_crop: Option<Crop>,
    /// "What I know about this game" (stored as `archive_notes`).
    pub archive_notes: String,
    pub is_available: bool,
    pub date_added: String,
    pub date_modified: String,
    pub last_checked: Option<String>,
    pub tags: Vec<String>,
    pub journal_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum ArtworkChange {
    Keep,
    /// A newly chosen image (already converted by `images::process_artwork`),
    /// optionally with the part to show in the frame.
    #[serde(rename_all = "camelCase")]
    Set {
        artwork_path: String,
        thumbnail_path: String,
        #[serde(default)]
        crop: Option<Crop>,
    },
    Remove,
    /// The same artwork, another part of it in the frame.
    Crop { crop: Crop },
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GameInput {
    pub title: String,
    /// Absolute path picked by the user (a stored portable path is accepted too).
    pub folder_path: String,
    pub description: String,
    pub developer: String,
    pub publisher: String,
    pub release_year: Option<i64>,
    pub genre: String,
    pub archive_notes: String,
    pub tags: Vec<String>,
    pub artwork: Option<ArtworkChange>,
}

/// Partial update (the notes panel saves on its own).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GamePatch {
    pub archive_notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LibraryQuery {
    /// Title search (case and diacritics insensitive).
    pub search: Option<String>,
    /// title | year | added | size
    pub sort: Option<String>,
    /// Filter: games with any of these genres (the genre field holds several,
    /// separated by commas).
    pub genres: Vec<String>,
    /// Filter: games with any of these tags. With genres too, a game needs
    /// both: one of the genres and one of the tags.
    pub tags: Vec<String>,
}

/// The genres in a genre field: "Role-playing, Strategy" -> both.
pub fn split_genres(genre: &str) -> Vec<String> {
    genre.split(',').map(text::tidy).filter(|g| !g.is_empty()).collect()
}

/// Comparison key for genre and tag names (case and diacritics insensitive).
fn name_key(s: &str) -> String {
    text::search_key(&text::tidy(s))
}

/// Ids of the games that pass the genre and tag filter (`None` = no filter).
fn filtered_ids(conn: &Connection, q: &LibraryQuery) -> Result<Option<std::collections::HashSet<i64>>> {
    let genres: std::collections::HashSet<String> = q.genres.iter().map(|g| name_key(g)).filter(|g| !g.is_empty()).collect();
    let tags: std::collections::HashSet<String> = q.tags.iter().map(|t| name_key(t)).filter(|t| !t.is_empty()).collect();
    if genres.is_empty() && tags.is_empty() {
        return Ok(None);
    }
    let mut by_genre: Option<std::collections::HashSet<i64>> = None;
    if !genres.is_empty() {
        let mut stmt = conn.prepare("SELECT id, genre FROM games WHERE genre <> ''")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut ids = std::collections::HashSet::new();
        for row in rows {
            let (id, genre) = row?;
            if split_genres(&genre).iter().any(|g| genres.contains(&name_key(g))) {
                ids.insert(id);
            }
        }
        by_genre = Some(ids);
    }
    let mut by_tag: Option<std::collections::HashSet<i64>> = None;
    if !tags.is_empty() {
        let mut stmt = conn.prepare("SELECT gt.game_id, t.name FROM game_tags gt JOIN tags t ON t.id = gt.tag_id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut ids = std::collections::HashSet::new();
        for row in rows {
            let (id, name) = row?;
            if tags.contains(&name_key(&name)) {
                ids.insert(id);
            }
        }
        by_tag = Some(ids);
    }
    Ok(Some(match (by_genre, by_tag) {
        (Some(g), Some(t)) => g.intersection(&t).copied().collect(),
        (Some(g), None) => g,
        (None, Some(t)) => t,
        (None, None) => unreachable!(),
    }))
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFacets {
    pub total: i64,
    pub unavailable: i64,
    pub genres: Vec<NameCount>,
    pub tags: Vec<NameCount>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NameCount {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalSummary {
    pub title: String,
    pub journal_entries_removed: i64,
    pub file_records_removed: i64,
    pub folder_untouched: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalPreview {
    pub title: String,
    pub folder: String,
    pub journal_entries: i64,
    pub file_records: i64,
    /// Screenshot copies stored in the archive (deleted with the game).
    pub screenshots: i64,
    /// Additional files copied into the archive (deleted with the game).
    pub addition_copies: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilityReport {
    pub total: i64,
    pub available: i64,
    pub unavailable: i64,
    pub unavailable_titles: Vec<String>,
    pub checked_at: String,
}

// ---------------------------------------------------------------- validation

fn clean_text(s: &str, max: usize, field: &str) -> Result<String> {
    let t = s.trim();
    if t.chars().count() > max {
        return Err(CoreError::invalid(format!("{field} is too long (maximum {max} characters).")));
    }
    Ok(t.to_string())
}

fn validate_year(y: Option<i64>) -> Result<Option<i64>> {
    match y {
        None => Ok(None),
        Some(y) if (1940..=2100).contains(&y) => Ok(Some(y)),
        Some(_) => Err(CoreError::invalid("Release year must be between 1940 and 2100.")),
    }
}

/// Validates the folder and returns `(stored_path, key)`.
fn prepare_folder(layout: &ArchiveLayout, raw: &str, must_exist: bool) -> Result<(String, String)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(CoreError::invalid("Please choose the game folder."));
    }
    let resolved = paths::resolve(&layout.root, raw);
    if must_exist && !resolved.is_dir() {
        return Err(CoreError::invalid(format!(
            "The folder does not exist or is not reachable: {}",
            paths::display(&resolved)
        )));
    }
    let stored = paths::to_portable(&layout.root, &resolved);
    let key = paths::folder_key(&stored);
    let data_key = paths::folder_key("Data");
    if key == "." || key == data_key || key.starts_with(&format!("{data_key}/")) {
        return Err(CoreError::invalid(
            "This folder belongs to Game Curator itself. Please choose the folder of the game.",
        ));
    }
    Ok((stored, key))
}

fn ensure_unique_folder(conn: &Connection, key: &str, except_id: Option<i64>) -> Result<()> {
    let existing: Option<(i64, String)> = conn
        .query_row("SELECT id, title FROM games WHERE folder_key = ?1", [key], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    if let Some((id, title)) = existing {
        if Some(id) != except_id {
            return Err(CoreError::invalid(format!("This folder is already assigned to \u{201c}{title}\u{201d}.")));
        }
    }
    Ok(())
}

fn validate_artwork_paths(layout: &ArchiveLayout, art: &str, thumb: &str) -> Result<()> {
    let ok = |p: &str, prefix: &str| {
        paths::is_safe_relative(p) && p.starts_with(prefix) && layout.root.join(p).is_file()
    };
    if ok(art, "Data/Artwork/Covers/") && ok(thumb, "Data/Thumbnails/") {
        Ok(())
    } else {
        Err(CoreError::invalid("The artwork file is missing. Please choose the image again."))
    }
}

pub fn normalize_tags(tags: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        for part in t.split(',') {
            let tag = text::tidy(part);
            if tag.is_empty() {
                continue;
            }
            if tag.chars().count() > 40 {
                return Err(CoreError::invalid(format!("Tag \u{201c}{tag}\u{201d} is too long (maximum 40 characters).")));
            }
            if !out.iter().any(|o| o.to_lowercase() == tag.to_lowercase()) {
                out.push(tag);
            }
        }
    }
    if out.len() > 30 {
        return Err(CoreError::invalid("A game can have at most 30 tags."));
    }
    Ok(out)
}

pub(crate) fn set_tags(tx: &Transaction, game_id: i64, tags: &[String]) -> Result<()> {
    tx.execute("DELETE FROM game_tags WHERE game_id = ?1", [game_id])?;
    for tag in tags {
        tx.execute("INSERT INTO tags (name) VALUES (?1) ON CONFLICT(name) DO NOTHING", [tag])?;
        let tag_id: i64 = tx.query_row("SELECT id FROM tags WHERE name = ?1", [tag], |r| r.get(0))?;
        tx.execute("INSERT OR IGNORE INTO game_tags (game_id, tag_id) VALUES (?1, ?2)", params![game_id, tag_id])?;
    }
    tx.execute("DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM game_tags)", [])?;
    Ok(())
}

pub fn game_tags(conn: &Connection, game_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.name FROM tags t JOIN game_tags gt ON gt.tag_id = t.id WHERE gt.game_id = ?1 ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([game_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------- CRUD

struct Prepared {
    title: String,
    folder: String,
    key: String,
    description: String,
    developer: String,
    publisher: String,
    year: Option<i64>,
    genre: String,
    notes: String,
    tags: Vec<String>,
}

fn prepare_input(layout: &ArchiveLayout, input: &GameInput, folder_must_exist: bool) -> Result<Prepared> {
    let title = clean_text(&input.title, 200, "Title")?;
    if title.is_empty() {
        return Err(CoreError::invalid("Please enter the game title."));
    }
    let (folder, key) = prepare_folder(layout, &input.folder_path, folder_must_exist)?;
    Ok(Prepared {
        title,
        folder,
        key,
        description: clean_text(&input.description, 20_000, "Description")?,
        developer: clean_text(&input.developer, 200, "Developer")?,
        publisher: clean_text(&input.publisher, 200, "Publisher")?,
        year: validate_year(input.release_year)?,
        genre: clean_text(&input.genre, 100, "Genre")?,
        notes: clean_text(&input.archive_notes, 50_000, "Notes")?,
        tags: normalize_tags(&input.tags)?,
    })
}

pub fn create_game(conn: &mut Connection, layout: &ArchiveLayout, input: &GameInput) -> Result<i64> {
    let p = prepare_input(layout, input, true)?;
    ensure_unique_folder(conn, &p.key, None)?;
    let (art, mut thumb, crop) = match &input.artwork {
        Some(ArtworkChange::Set { artwork_path, thumbnail_path, crop }) => {
            validate_artwork_paths(layout, artwork_path, thumbnail_path)?;
            (Some(artwork_path.clone()), Some(thumbnail_path.clone()), *crop)
        }
        _ => (None, None, None),
    };
    // the thumbnail made when the image was chosen is replaced by the chosen crop
    let mut superseded_thumb = None;
    if let (Some(a), Some(c)) = (&art, &crop) {
        superseded_thumb = thumb.replace(images::rerender_thumbnail(layout, a, c)?);
    }
    let tx = conn.transaction()?;
    let now = db::now_iso(&tx)?;
    tx.execute(
        "INSERT INTO games (title, title_search, folder_path, folder_key, description, developer, publisher,
                            release_year, genre, artwork_path, thumbnail_path, archive_notes, is_available,
                            date_added, date_modified, last_checked, cover_crop)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, ?13, ?13, ?13, ?14)",
        params![
            p.title,
            text::search_key(&p.title),
            p.folder,
            p.key,
            p.description,
            p.developer,
            p.publisher,
            p.year,
            p.genre,
            art,
            thumb,
            p.notes,
            now,
            crop.map(|c| c.to_json())
        ],
    )?;
    let id = tx.last_insert_rowid();
    set_tags(&tx, id, &p.tags)?;
    tx.commit()?;
    if let Some(old) = superseded_thumb {
        images::remove_artwork_files(layout, None, Some(&old));
    }
    Ok(id)
}

pub fn update_game(conn: &mut Connection, layout: &ArchiveLayout, id: i64, input: &GameInput) -> Result<()> {
    type Current = (String, Option<String>, Option<String>, Option<String>);
    let current: Option<Current> = conn
        .query_row("SELECT folder_key, artwork_path, thumbnail_path, cover_crop FROM games WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .optional()?;
    let (old_key, old_art, old_thumb, old_crop) =
        current.ok_or_else(|| CoreError::not_found("The game no longer exists."))?;

    // Folder must exist only when the user changes it (an unchanged folder may be
    // on a currently disconnected drive - that must not block editing).
    let tentative = prepare_input(layout, input, false)?;
    let p = if tentative.key != old_key { prepare_input(layout, input, true)? } else { tentative };
    ensure_unique_folder(conn, &p.key, Some(id))?;

    // (artwork, thumbnail, crop, old artwork replaced, a thumbnail no longer used)
    let (art, thumb, crop, remove_old, superseded_thumb) = match &input.artwork {
        Some(ArtworkChange::Set { artwork_path, thumbnail_path, crop }) => {
            validate_artwork_paths(layout, artwork_path, thumbnail_path)?;
            let mut t = thumbnail_path.clone();
            let mut superseded = None;
            if let Some(c) = crop {
                superseded = Some(std::mem::replace(&mut t, images::rerender_thumbnail(layout, artwork_path, c)?));
            }
            (Some(artwork_path.clone()), Some(t), crop.map(|c| c.to_json()), true, superseded)
        }
        Some(ArtworkChange::Remove) => (None, None, None, true, None),
        Some(ArtworkChange::Crop { crop }) => {
            let a = old_art.clone().ok_or_else(|| CoreError::invalid("This game has no artwork to adjust."))?;
            let t = images::rerender_thumbnail(layout, &a, crop)?;
            (Some(a), Some(t), Some(crop.to_json()), false, old_thumb.clone())
        }
        Some(ArtworkChange::Keep) | None => (old_art.clone(), old_thumb.clone(), old_crop, false, None),
    };

    let tx = conn.transaction()?;
    let now = db::now_iso(&tx)?;
    tx.execute(
        "UPDATE games SET title = ?1, title_search = ?2, folder_path = ?3, folder_key = ?4, description = ?5,
                developer = ?6, publisher = ?7, release_year = ?8, genre = ?9, artwork_path = ?10,
                thumbnail_path = ?11, archive_notes = ?12, date_modified = ?13, cover_crop = ?15
         WHERE id = ?14",
        params![
            p.title,
            text::search_key(&p.title),
            p.folder,
            p.key,
            p.description,
            p.developer,
            p.publisher,
            p.year,
            p.genre,
            art,
            thumb,
            p.notes,
            now,
            id,
            crop
        ],
    )?;
    if p.key != old_key {
        // A different folder: the old file catalog no longer applies.
        tx.execute("DELETE FROM files WHERE game_id = ?1", [id])?;
        let available = paths::resolve(&layout.root, &p.folder).is_dir();
        tx.execute("UPDATE games SET is_available = ?1, last_checked = ?2 WHERE id = ?3", params![available as i64, now, id])?;
    }
    set_tags(&tx, id, &p.tags)?;
    tx.commit()?;

    if remove_old {
        let new_art = art.as_deref();
        if old_art.as_deref() != new_art {
            images::remove_artwork_files(layout, old_art.as_deref(), old_thumb.as_deref());
        }
    }
    if let Some(t) = superseded_thumb.filter(|t| Some(t.as_str()) != thumb.as_deref()) {
        images::remove_artwork_files(layout, None, Some(&t));
    }
    Ok(())
}

/// Shows another part of the game's artwork in the frame (the artwork
/// editor in the game profile). Only the thumbnail is rendered again.
pub fn set_cover_crop(conn: &mut Connection, layout: &ArchiveLayout, id: i64, crop: &Crop) -> Result<String> {
    let (art, old_thumb): (Option<String>, Option<String>) = conn
        .query_row("SELECT artwork_path, thumbnail_path FROM games WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| CoreError::not_found("The game no longer exists."))?;
    let art = art.ok_or_else(|| CoreError::invalid("This game has no artwork to adjust."))?;
    let thumb = images::rerender_thumbnail(layout, &art, crop)?;
    let now = db::now_iso(conn)?;
    conn.execute(
        "UPDATE games SET thumbnail_path = ?1, cover_crop = ?2, date_modified = ?3 WHERE id = ?4",
        params![thumb, crop.to_json(), now, id],
    )?;
    if let Some(old) = old_thumb {
        images::remove_artwork_files(layout, None, Some(&old));
    }
    Ok(thumb)
}

pub fn patch_game(conn: &mut Connection, id: i64, patch: &GamePatch) -> Result<()> {
    let tx = conn.transaction()?;
    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM games WHERE id = ?1)", [id], |r| r.get(0))?;
    if !exists {
        return Err(CoreError::not_found("The game no longer exists."));
    }
    let now = db::now_iso(&tx)?;
    if let Some(n) = &patch.archive_notes {
        let n = clean_text(n, 50_000, "Notes")?;
        tx.execute("UPDATE games SET archive_notes = ?1, date_modified = ?2 WHERE id = ?3", params![n, now, id])?;
    }
    tx.commit()?;
    Ok(())
}

pub fn get_game(conn: &Connection, layout: &ArchiveLayout, id: i64) -> Result<GameDetail> {
    let detail = conn
        .query_row(
            "SELECT g.id, g.title, g.folder_path, g.description, g.developer, g.publisher, g.release_year, g.genre,
                    g.artwork_path, g.thumbnail_path, g.archive_notes,
                    g.is_available, g.date_added, g.date_modified, g.last_checked,
                    (SELECT COUNT(*) FROM journal_entries j WHERE j.game_id = g.id), g.cover_crop
             FROM games g WHERE g.id = ?1",
            [id],
            |r| {
                let folder_path: String = r.get(2)?;
                let resolved = paths::resolve(&layout.root, &folder_path);
                Ok(GameDetail {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    folder_resolved: paths::display(&resolved),
                    folder_portable: paths::is_portable(&folder_path),
                    folder_exists: resolved.is_dir(),
                    folder_path,
                    description: r.get(3)?,
                    developer: r.get(4)?,
                    publisher: r.get(5)?,
                    release_year: r.get(6)?,
                    genre: r.get(7)?,
                    artwork_path: r.get(8)?,
                    thumbnail_path: r.get(9)?,
                    cover_crop: Crop::from_json(r.get::<_, Option<String>>(16)?.as_deref()),
                    archive_notes: r.get(10)?,
                    is_available: r.get::<_, i64>(11)? != 0,
                    date_added: r.get(12)?,
                    date_modified: r.get(13)?,
                    last_checked: r.get(14)?,
                    tags: Vec::new(),
                    journal_count: r.get(15)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::not_found("The game no longer exists."))?;
    let mut detail = detail;
    detail.tags = game_tags(conn, id)?;
    Ok(detail)
}

pub fn removal_preview(conn: &Connection, layout: &ArchiveLayout, id: i64) -> Result<RemovalPreview> {
    conn.query_row(
        "SELECT title, folder_path,
                (SELECT COUNT(*) FROM journal_entries WHERE game_id = ?1),
                (SELECT COUNT(*) FROM files WHERE game_id = ?1),
                (SELECT COUNT(*) FROM screenshots WHERE game_id = ?1),
                (SELECT COUNT(*) FROM additions WHERE game_id = ?1 AND kind = 'copy')
         FROM games WHERE id = ?1",
        [id],
        |r| {
            let folder: String = r.get(1)?;
            Ok(RemovalPreview {
                title: r.get(0)?,
                folder: paths::display(&paths::resolve(&layout.root, &folder)),
                journal_entries: r.get(2)?,
                file_records: r.get(3)?,
                screenshots: r.get(4)?,
                addition_copies: r.get(5)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::not_found("The game no longer exists."))
}

/// Removes the game record and its dependent records (journal entries, file
/// records, tag links) after the user confirmed it. The game folder and all
/// its files stay exactly where they are.
pub fn remove_game(conn: &mut Connection, layout: &ArchiveLayout, id: i64) -> Result<RemovalSummary> {
    remove_game_with(conn, layout, id, &RemoveOptions::default())
}

/// What else goes when a game is removed. By default the screenshot copies
/// and the copied additional files stay in `Data/` (nothing the user made is
/// lost); the user may choose to delete them.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RemoveOptions {
    pub delete_screenshots: bool,
    pub delete_addition_copies: bool,
}

pub fn remove_game_with(conn: &mut Connection, layout: &ArchiveLayout, id: i64, opts: &RemoveOptions) -> Result<RemovalSummary> {
    let preview = removal_preview(conn, layout, id)?;
    let (art, thumb, icon): (Option<String>, Option<String>, Option<String>) = conn.query_row(
        "SELECT artwork_path, thumbnail_path, icon_path FROM games WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let shots = crate::screenshots::files_of_game(conn, id)?;
    let copies = crate::additions::copies_of_game(conn, layout, id)?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM games WHERE id = ?1", [id])?;
    tx.execute("DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM game_tags)", [])?;
    tx.commit()?;
    images::remove_artwork_files(layout, art.as_deref(), thumb.as_deref());
    images::remove_artwork_files(layout, None, icon.as_deref());
    // copies Game Curator made for this game, only when the user chose so;
    // the player's originals, linked files and the game folder stay untouched
    if opts.delete_screenshots {
        for (img, th) in &shots {
            images::remove_screenshot_files(layout, img, th);
        }
    }
    if opts.delete_addition_copies {
        for c in &copies {
            let _ = std::fs::remove_file(c);
        }
    }
    let _ = std::fs::remove_dir(layout.screenshots_dir().join(format!("game-{id}")));
    let _ = std::fs::remove_dir(layout.additions_dir().join(format!("game-{id}")));
    Ok(RemovalSummary {
        title: preview.title,
        journal_entries_removed: preview.journal_entries,
        file_records_removed: preview.file_records,
        folder_untouched: preview.folder,
    })
}

// ---------------------------------------------------------------- queries

fn card_from_row(r: &Row) -> rusqlite::Result<GameCard> {
    Ok(GameCard {
        id: r.get(0)?,
        title: r.get(1)?,
        developer: r.get(2)?,
        genre: r.get(3)?,
        release_year: r.get(4)?,
        is_available: r.get::<_, i64>(5)? != 0,
        thumbnail_path: r.get(6)?,
        folder_path: r.get(7)?,
        file_count: r.get(8)?,
        total_size: r.get(9)?,
        is_installed: r.get::<_, i64>(10)? != 0,
        icon_path: r.get(11)?,
    })
}

/// The library. `machine` is this computer's name: it decides which games
/// count as installed.
pub fn list_games(conn: &Connection, q: &LibraryQuery, machine: &str) -> Result<Vec<GameCard>> {
    let mut sql = String::from(
        "SELECT g.id, g.title, g.developer, g.genre, g.release_year, g.is_available, g.thumbnail_path, g.folder_path,
                COALESCE(f.n, 0), COALESCE(f.bytes, 0),
                EXISTS(SELECT 1 FROM installations i WHERE i.game_id = g.id AND i.machine = ?), g.icon_path
         FROM games g
         LEFT JOIN (SELECT game_id, COUNT(*) AS n, SUM(size_bytes) AS bytes FROM files WHERE is_missing = 0 GROUP BY game_id) f
                ON f.game_id = g.id",
    );
    let mut args: Vec<Value> = vec![Value::Text(machine.to_string())];
    if let Some(search) = q.search.as_deref().map(text::search_key).filter(|s| !s.is_empty()) {
        sql.push_str(" WHERE g.title_search LIKE ? ESCAPE '\\'");
        args.push(Value::Text(format!("%{}%", text::escape_like(&search))));
    }
    sql.push_str(match q.sort.as_deref().unwrap_or("title") {
        "year" => " ORDER BY g.release_year IS NULL, g.release_year DESC, g.title_search",
        "added" => " ORDER BY g.date_added DESC, g.id DESC",
        "size" => " ORDER BY COALESCE(f.bytes, 0) DESC, g.title_search",
        _ => " ORDER BY g.title_search, g.id",
    });
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter()), card_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(match filtered_ids(conn, q)? {
        Some(ids) => rows.into_iter().filter(|g| ids.contains(&g.id)).collect(),
        None => rows,
    })
}

pub fn library_facets(conn: &Connection) -> Result<LibraryFacets> {
    let mut f = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(is_available = 0), 0) FROM games",
        [],
        |r| {
            Ok(LibraryFacets {
                total: r.get(0)?,
                unavailable: r.get(1)?,
                genres: Vec::new(),
                tags: Vec::new(),
            })
        },
    )?;
    // each genre of "Role-playing, Strategy" counts on its own; "RPG" and
    // "rpg" are one genre (shown as first written)
    let mut stmt = conn.prepare("SELECT genre FROM games WHERE genre <> '' ORDER BY id")?;
    let mut genres: Vec<(String, NameCount)> = Vec::new();
    for genre in stmt.query_map([], |r| r.get::<_, String>(0))? {
        let genre = genre?;
        let mut seen = std::collections::HashSet::new();
        for g in split_genres(&genre) {
            let key = name_key(&g);
            if !seen.insert(key.clone()) {
                continue;
            }
            match genres.iter_mut().find(|(k, _)| *k == key) {
                Some((_, nc)) => nc.count += 1,
                None => genres.push((key, NameCount { name: g, count: 1 })),
            }
        }
    }
    genres.sort_by(|a, b| a.0.cmp(&b.0));
    f.genres = genres.into_iter().map(|(_, nc)| nc).collect();
    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(gt.game_id) FROM tags t JOIN game_tags gt ON gt.tag_id = t.id
         GROUP BY t.id ORDER BY t.name COLLATE NOCASE",
    )?;
    f.tags = stmt
        .query_map([], |r| Ok(NameCount { name: r.get(0)?, count: r.get(1)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(f)
}

/// Simple id/title list (journal filters, pickers).
pub fn list_titles(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare("SELECT id, title FROM games ORDER BY title_search")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn game_folder(conn: &Connection, layout: &ArchiveLayout, id: i64) -> Result<std::path::PathBuf> {
    let stored: String = conn
        .query_row("SELECT folder_path FROM games WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| CoreError::not_found("The game no longer exists."))?;
    Ok(paths::resolve(&layout.root, &stored))
}

pub fn game_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?)
}

/// Checks every game folder and stores the result. Records of unavailable
/// folders are kept untouched - they are only flagged.
pub fn check_availability(conn: &mut Connection, root: &Path) -> Result<AvailabilityReport> {
    let rows: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, title, folder_path FROM games ORDER BY title_search")?;
        let r = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        r
    };
    let tx = conn.transaction()?;
    let now = db::now_iso(&tx)?;
    let mut report = AvailabilityReport {
        total: rows.len() as i64,
        available: 0,
        unavailable: 0,
        unavailable_titles: Vec::new(),
        checked_at: now.clone(),
    };
    {
        let mut upd = tx.prepare("UPDATE games SET is_available = ?1, last_checked = ?2 WHERE id = ?3")?;
        for (id, title, folder) in rows {
            let ok = paths::resolve(root, &folder).is_dir();
            if ok {
                report.available += 1;
            } else {
                report.unavailable += 1;
                report.unavailable_titles.push(title);
            }
            upd.execute(params![ok as i64, now, id])?;
        }
    }
    tx.commit()?;
    Ok(report)
}
