//! Additional files of a game: compatibility fixes, community patches, mods,
//! bonus material (a making-of documentary, wallpapers...). Everything that
//! belongs to the game, in one place, so it never has to be searched for
//! again.
//!
//! A file is either *copied* into the archive (`Data/Additions/game-<id>/`,
//! travels with the drive) or *linked* where it already is (stored relative
//! to the archive root when it lies inside the archive, otherwise as an
//! absolute path). Linked files are never changed or deleted; copies are
//! deleted only when the user explicitly asks for it.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::archive::ArchiveLayout;
use crate::{db, paths, CoreError, Result};

pub const CATEGORIES: &[&str] = &["fix", "patch", "mod", "bonus", "other"];
const MAX_NOTE: usize = 2000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Addition {
    pub id: i64,
    pub game_id: i64,
    /// "copy" | "link"
    pub kind: String,
    /// Stored path (relative to the archive root, or absolute).
    pub path: String,
    /// The path for display (absolute on this computer).
    pub location: String,
    pub filename: String,
    /// Name shown in the catalog instead of the file name (the file keeps its name).
    pub display_name: Option<String>,
    pub category: String,
    pub note: String,
    pub size_bytes: i64,
    pub date_added: String,
    /// The file is there right now.
    pub exists: bool,
    /// The path follows the archive to another computer.
    pub portable: bool,
    /// A program (.exe/.msi): started with "Run" after a confirmation.
    pub runnable: bool,
    /// A file type that executes code when opened (never opened directly).
    pub risky: bool,
}

/// A file copied into the archive, not yet in the list.
#[derive(Debug, Clone)]
pub struct CopiedFile {
    pub rel: String,
    pub name: String,
    pub size: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AdditionInput {
    pub category: String,
    pub note: String,
    /// Name shown in the catalog: `None` keeps the current one, an empty
    /// string restores the file name. Only used when editing.
    pub display_name: Option<String>,
}

impl AdditionInput {
    fn validated(&self) -> Result<(String, String)> {
        let category = if self.category.is_empty() { "other".to_string() } else { self.category.clone() };
        if !CATEGORIES.contains(&category.as_str()) {
            return Err(CoreError::invalid("Unknown kind of file."));
        }
        let note = self.note.trim().to_string();
        if note.chars().count() > MAX_NOTE {
            return Err(CoreError::invalid(format!("The note is too long (maximum {MAX_NOTE} characters).")));
        }
        Ok((category, note))
    }
}

const SELECT: &str = "SELECT id, game_id, kind, path, filename, category, note, size_bytes, date_added, display_name FROM additions";

fn from_row(layout: &ArchiveLayout) -> impl Fn(&Row) -> rusqlite::Result<Addition> + '_ {
    move |r| {
        let path: String = r.get(3)?;
        let portable = !Path::new(&path).is_absolute() && paths::is_safe_relative(&path);
        let abs = if portable { layout.root.join(&path) } else { PathBuf::from(&path) };
        Ok(Addition {
            id: r.get(0)?,
            game_id: r.get(1)?,
            kind: r.get(2)?,
            location: paths::display(&abs),
            exists: abs.is_file(),
            portable,
            path,
            runnable: crate::files::is_runnable(&r.get::<_, String>(4)?),
            risky: crate::files::is_risky(&r.get::<_, String>(4)?),
            filename: r.get(4)?,
            category: r.get(5)?,
            note: r.get(6)?,
            size_bytes: r.get(7)?,
            date_added: r.get(8)?,
            display_name: r.get(9)?,
        })
    }
}

pub fn get(conn: &Connection, layout: &ArchiveLayout, id: i64) -> Result<Addition> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row(layout))
        .optional()?
        .ok_or_else(|| CoreError::not_found("The file is no longer in the list."))
}

pub fn list(conn: &Connection, layout: &ArchiveLayout, game_id: i64) -> Result<Vec<Addition>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE game_id = ?1
         ORDER BY CASE category WHEN 'fix' THEN 0 WHEN 'patch' THEN 1 WHEN 'mod' THEN 2 WHEN 'bonus' THEN 3 ELSE 4 END,
                  filename COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([game_id], from_row(layout))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Checks the game and the entered kind/note before anything is copied.
pub fn check(conn: &Connection, game_id: i64, input: &AdditionInput) -> Result<()> {
    check_game(conn, game_id)?;
    input.validated().map(|_| ())
}

fn check_game(conn: &Connection, game_id: i64) -> Result<()> {
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM games WHERE id = ?1)", [game_id], |r| r.get(0))?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::not_found("The game no longer exists."))
    }
}

fn check_source(source: &Path) -> Result<(String, i64)> {
    let meta = fs::metadata(source).map_err(|_| CoreError::invalid("The file cannot be read."))?;
    if !meta.is_file() {
        return Err(CoreError::invalid("Please choose a file."));
    }
    let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty());
    let name = name.ok_or_else(|| CoreError::invalid("Please choose a file."))?;
    Ok((name, meta.len() as i64))
}

/// A file name that is safe inside `Data/Additions/game-<id>/` and not taken
/// yet ("Fix.zip", "Fix (2).zip", ...).
fn free_name(dir: &Path, name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '_' } else { c })
        .collect();
    let clean = clean.trim().trim_end_matches(['.', ' ']).to_string();
    let clean = if clean.is_empty() || clean == ".." { "file".to_string() } else { clean };
    let (stem, ext) = match clean.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (clean.clone(), String::new()),
    };
    let taken = |n: &str| {
        let lower = n.to_lowercase();
        dir.join(n).exists()
            || fs::read_dir(dir).map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase() == lower)).unwrap_or(false)
    };
    if !taken(&clean) {
        return clean;
    }
    (2..10_000).map(|i| format!("{stem} ({i}){ext}")).find(|n| !taken(n)).unwrap_or_else(|| format!("{stem}-{}{ext}", uuid::Uuid::new_v4().simple()))
}

/// Copies a file into the archive (`Data/Additions/game-<id>/`). The
/// original stays where it is.
pub fn add_copy(conn: &Connection, layout: &ArchiveLayout, game_id: i64, source: &Path, input: &AdditionInput) -> Result<Addition> {
    check(conn, game_id, input)?;
    let copied = copy_file(layout, game_id, source)?;
    record_copy(conn, layout, game_id, &copied, input)
}

/// The copying itself (may take a while for big files; needs no database).
pub fn copy_file(layout: &ArchiveLayout, game_id: i64, source: &Path) -> Result<CopiedFile> {
    let (name, size) = check_source(source)?;
    let dir = layout.additions_dir().join(format!("game-{game_id}"));
    fs::create_dir_all(&dir)?;
    let target_name = free_name(&dir, &name);
    let target = dir.join(&target_name);
    let part = dir.join(format!("{target_name}.part"));
    let copied = fs::copy(source, &part).and_then(|n| {
        if n as i64 == size {
            fs::rename(&part, &target)
        } else {
            Err(std::io::Error::other("the copy is incomplete"))
        }
    });
    if let Err(e) = copied {
        let _ = fs::remove_file(&part);
        return Err(CoreError::invalid(format!("The file could not be copied into the archive: {e}")));
    }
    Ok(CopiedFile { rel: format!("Data/Additions/game-{game_id}/{target_name}"), name: target_name, size })
}

/// Adds a copied file to the list (removes the copy if that fails).
pub fn record_copy(conn: &Connection, layout: &ArchiveLayout, game_id: i64, copied: &CopiedFile, input: &AdditionInput) -> Result<Addition> {
    let inserted = check(conn, game_id, input).and_then(|_| {
        let (category, note) = input.validated()?;
        let now = db::now_iso(conn)?;
        conn.execute(
            "INSERT INTO additions (game_id, kind, path, filename, category, note, size_bytes, date_added)
             VALUES (?1, 'copy', ?2, ?3, ?4, ?5, ?6, ?7)",
            params![game_id, copied.rel, copied.name, category, note, copied.size, now],
        )?;
        Ok(())
    });
    if let Err(e) = inserted {
        if paths::is_safe_relative(&copied.rel) && copied.rel.starts_with("Data/Additions/") {
            let _ = fs::remove_file(layout.root.join(&copied.rel));
        }
        return Err(e);
    }
    get(conn, layout, conn.last_insert_rowid())
}

/// Remembers a file where it already is. Inside the archive the path is
/// stored relative to the archive root (follows the drive), elsewhere as an
/// absolute path.
pub fn add_link(conn: &Connection, layout: &ArchiveLayout, game_id: i64, source: &Path, input: &AdditionInput) -> Result<Addition> {
    check_game(conn, game_id)?;
    let (category, note) = input.validated()?;
    let (name, size) = check_source(source)?;
    if !source.is_absolute() {
        return Err(CoreError::invalid("Please choose a file."));
    }
    let stored = match paths::relative_to(&layout.root, source).map(|r| paths::to_slash(&r)) {
        Some(rel) if paths::is_safe_relative(&rel) => rel,
        _ => paths::display(source),
    };
    let key = stored.to_lowercase().replace('\\', "/");
    let dup: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM additions WHERE game_id = ?1 AND lower(replace(path, '\\', '/')) = ?2)",
        params![game_id, key],
        |r| r.get(0),
    )?;
    if dup {
        return Err(CoreError::invalid("This file is already in the list."));
    }
    let now = db::now_iso(conn)?;
    conn.execute(
        "INSERT INTO additions (game_id, kind, path, filename, category, note, size_bytes, date_added)
         VALUES (?1, 'link', ?2, ?3, ?4, ?5, ?6, ?7)",
        params![game_id, stored, name, category, note, size, now],
    )?;
    get(conn, layout, conn.last_insert_rowid())
}

pub fn update(conn: &Connection, layout: &ArchiveLayout, id: i64, input: &AdditionInput) -> Result<Addition> {
    let (category, note) = input.validated()?;
    let filename: String = conn
        .query_row("SELECT filename FROM additions WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| CoreError::not_found("The file is no longer in the list."))?;
    // only the name in the catalog changes: the file itself is never renamed
    let name = match input.display_name.as_deref() {
        Some(raw) => Some(crate::text::display_name(raw, &filename)?),
        None => None,
    };
    conn.execute("UPDATE additions SET category = ?1, note = ?2 WHERE id = ?3", params![category, note, id])?;
    if let Some(clean) = name {
        conn.execute("UPDATE additions SET display_name = ?1 WHERE id = ?2", params![clean, id])?;
    }
    get(conn, layout, id)
}

/// Path of a copy that Game Curator itself made (the only files it may delete).
fn own_copy(layout: &ArchiveLayout, a: &Addition) -> Option<PathBuf> {
    (a.kind == "copy" && paths::is_safe_relative(&a.path) && a.path.starts_with("Data/Additions/")).then(|| layout.root.join(&a.path))
}

/// Takes a file off the list (after the user confirmed it). A linked file is
/// never touched; a copy in the archive is deleted only with `delete_copy`.
pub fn remove(conn: &Connection, layout: &ArchiveLayout, id: i64, delete_copy: bool) -> Result<Addition> {
    let a = get(conn, layout, id)?;
    conn.execute("DELETE FROM additions WHERE id = ?1", [id])?;
    if delete_copy {
        if let Some(p) = own_copy(layout, &a) {
            let _ = fs::remove_file(p);
        }
    }
    Ok(a)
}

/// Copies made by Game Curator for a game (deleted together with the game).
pub fn copies_of_game(conn: &Connection, layout: &ArchiveLayout, game_id: i64) -> Result<Vec<PathBuf>> {
    Ok(list(conn, layout, game_id)?.iter().filter_map(|a| own_copy(layout, a)).collect())
}

/// The file on this computer, verified to exist (for Open / Show in folder).
pub fn file(conn: &Connection, layout: &ArchiveLayout, id: i64) -> Result<PathBuf> {
    let a = get(conn, layout, id)?;
    let p = if a.portable { layout.root.join(&a.path) } else { PathBuf::from(&a.path) };
    if !p.is_file() {
        return Err(CoreError::not_found(format!("The file was not found:\n{}", a.location)));
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_names() {
        let dir = std::env::temp_dir().join(format!("gc-addtest-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_name(&dir, "Fix.zip"), "Fix.zip");
        fs::write(dir.join("Fix.zip"), b"x").unwrap();
        assert_eq!(free_name(&dir, "fix.ZIP"), "fix (2).ZIP");
        assert_eq!(free_name(&dir, "a:b?.txt"), "a_b_.txt");
        assert_eq!(free_name(&dir, ".."), "file");
        let _ = fs::remove_dir_all(dir);
    }
}
