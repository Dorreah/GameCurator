//! Archive content of a game folder: listing, categorization, file catalog
//! (custom categories, primary installer) and safe path resolution for the
//! Open / Run actions.
//!
//! Nothing here ever modifies, moves or deletes files of the game folder.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::archive::ArchiveLayout;
use crate::{db, games, paths, CoreError, Result};

pub const CATEGORIES: &[&str] = &[
    "installer", "executable", "data", "manual", "image", "music", "archive", "text", "other",
];

/// Listing limits keep the profile fast even if a user points a game at a
/// huge installed-game directory.
pub const MAX_DEPTH: usize = 4;
pub const MAX_ENTRIES: usize = 3000;

pub fn extension(name: &str) -> String {
    Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Category guessed from the file name. An `.exe` is only called an
/// installer when its name suggests so - everything else is "executable".
pub fn classify(name: &str) -> &'static str {
    let ext = extension(name);
    match ext.as_str() {
        "msi" => "installer",
        "exe" => {
            let lower = name.to_lowercase();
            if lower.starts_with("setup") || lower.contains("install") || lower.starts_with("patch_") {
                "installer"
            } else {
                "executable"
            }
        }
        "bin" => "data",
        "pdf" => "manual",
        "jpg" | "jpeg" | "png" | "webp" => "image",
        "mp3" | "flac" | "ogg" | "wav" => "music",
        "zip" | "7z" | "rar" => "archive",
        "txt" | "md" => "text",
        _ => "other",
    }
}

pub fn is_runnable(name: &str) -> bool {
    matches!(extension(name).as_str(), "exe" | "msi")
}

/// File types that execute code when "opened" by Windows. `Open` refuses
/// them; `.exe`/`.msi` can be started with `Run` after confirmation, scripts
/// and shortcuts are not started at all.
pub fn is_risky(name: &str) -> bool {
    matches!(
        extension(name).as_str(),
        "exe" | "msi" | "bat" | "cmd" | "com" | "scr" | "pif" | "ps1" | "psm1" | "vbs" | "vbe" | "js" | "jse"
            | "wsf" | "wsh" | "hta" | "lnk" | "url" | "cpl" | "msc" | "jar" | "reg" | "appx" | "msix" | "inf"
            | "application" | "gadget" | "msp" | "mst"
    )
}

fn is_ignored(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "thumbs.db" || lower == "desktop.ini" || lower == ".ds_store" || lower.starts_with("~$")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsDir {
    pub rel_path: String,
    pub name: String,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsFile {
    pub rel_path: String,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderListing {
    pub dirs: Vec<FsDir>,
    pub files: Vec<FsFile>,
    pub truncated: bool,
}

fn join_rel(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

/// Lists a folder recursively (sorted, depth- and size-limited). Symlinked
/// directories are not followed.
pub fn list_folder(root: &Path, max_depth: usize, max_entries: usize) -> Result<FolderListing> {
    let mut listing = FolderListing::default();
    walk(root, "", 1, max_depth, max_entries, &mut listing)?;
    Ok(listing)
}

fn walk(dir: &Path, prefix: &str, depth: usize, max_depth: usize, max_entries: usize, out: &mut FolderListing) -> Result<()> {
    let mut entries: Vec<(String, fs::FileType, PathBuf)> = Vec::new();
    for e in fs::read_dir(dir)? {
        let Ok(e) = e else { continue };
        let Ok(ft) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if is_ignored(&name) {
            continue;
        }
        entries.push((name, ft, e.path()));
    }
    entries.sort_by(|a, b| {
        b.1.is_dir().cmp(&a.1.is_dir()).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    for (name, ft, path) in entries {
        if out.dirs.len() + out.files.len() >= max_entries {
            out.truncated = true;
            return Ok(());
        }
        let rel = join_rel(prefix, &name);
        if ft.is_dir() {
            out.dirs.push(FsDir { rel_path: rel.clone(), name, depth });
            if depth < max_depth {
                // Unreadable subfolders (permissions) are skipped, not fatal.
                let _ = walk(&path, &rel, depth + 1, max_depth, max_entries, out);
            } else {
                out.truncated = true;
            }
        } else if ft.is_file() {
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            out.files.push(FsFile { rel_path: rel, name, size });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub id: i64,
    /// Opening would execute code (scripts, shortcuts...).
    pub risky: bool,
    pub file_path: String,
    pub filename: String,
    pub file_type: String,
    pub custom_category: Option<String>,
    pub category: String,
    pub is_primary: bool,
    pub size_bytes: Option<i64>,
    pub is_missing: bool,
    pub runnable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveContent {
    pub game_id: i64,
    pub folder: String,
    pub folder_available: bool,
    pub dirs: Vec<FsDir>,
    pub files: Vec<FileRecord>,
    pub truncated: bool,
    pub refreshed: bool,
}

/// Brings the `files` table in line with the folder listing. Files that
/// disappeared are flagged `is_missing` (never deleted); a truncated listing
/// does not flag anything because it is incomplete.
pub fn sync_files(conn: &mut Connection, game_id: i64, listing: &FolderListing) -> Result<()> {
    let tx = conn.transaction()?;
    let now = db::now_iso(&tx)?;
    if !listing.truncated {
        // Everything is presumed missing until it is seen in the listing.
        tx.execute("UPDATE files SET is_missing = 1 WHERE game_id = ?1", [game_id])?;
    }
    {
        let mut up = tx.prepare_cached(
            "INSERT INTO files (game_id, file_path, filename, file_type, size_bytes, is_missing, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)
             ON CONFLICT(game_id, file_path) DO UPDATE SET
                filename = excluded.filename, file_type = excluded.file_type,
                size_bytes = excluded.size_bytes, is_missing = 0, last_seen = excluded.last_seen",
        )?;
        for f in &listing.files {
            up.execute(params![game_id, f.rel_path, f.name, classify(&f.name), f.size as i64, now])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn file_records(conn: &Connection, game_id: i64) -> Result<Vec<FileRecord>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, file_path, filename, file_type, custom_category, is_primary, size_bytes, is_missing
         FROM files WHERE game_id = ?1",
    )?;
    let mut rows = stmt
        .query_map([game_id], |r| {
            let filename: String = r.get(2)?;
            let file_type: String = r.get(3)?;
            let custom: Option<String> = r.get(4)?;
            Ok(FileRecord {
                id: r.get(0)?,
                file_path: r.get(1)?,
                runnable: is_runnable(&filename),
                risky: is_risky(&filename),
                filename,
                category: custom.clone().unwrap_or_else(|| file_type.clone()),
                file_type,
                custom_category: custom,
                is_primary: r.get::<_, i64>(5)? != 0,
                size_bytes: r.get(6)?,
                is_missing: r.get::<_, i64>(7)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // Primary first, then top-level files before subfolders, then by path.
    rows.sort_by(|a, b| {
        b.is_primary
            .cmp(&a.is_primary)
            .then_with(|| a.file_path.contains('/').cmp(&b.file_path.contains('/')))
            .then_with(|| a.file_path.to_lowercase().cmp(&b.file_path.to_lowercase()))
    });
    Ok(rows)
}

/// Reads the game folder (when reachable), syncs the catalog and returns
/// everything the "Archive Content" panel needs.
pub fn archive_content(conn: &mut Connection, layout: &ArchiveLayout, game_id: i64) -> Result<ArchiveContent> {
    let folder = games::game_folder(conn, layout, game_id)?;
    let available = folder.is_dir();
    let mut dirs = Vec::new();
    let mut truncated = false;
    let mut refreshed = false;
    if available {
        if let Ok(listing) = list_folder(&folder, MAX_DEPTH, MAX_ENTRIES) {
            sync_files(conn, game_id, &listing)?;
            dirs = listing.dirs;
            truncated = listing.truncated;
            refreshed = true;
        }
    }
    let now = db::now_iso(conn)?;
    conn.execute(
        "UPDATE games SET is_available = ?1, last_checked = ?2 WHERE id = ?3",
        params![available as i64, now, game_id],
    )?;
    Ok(ArchiveContent {
        game_id,
        folder: paths::display(&folder),
        folder_available: available,
        dirs,
        files: file_records(conn, game_id)?,
        truncated,
        refreshed,
    })
}

pub fn set_category(conn: &Connection, game_id: i64, file_id: i64, category: Option<&str>) -> Result<()> {
    let category = match category.map(str::trim).filter(|c| !c.is_empty() && *c != "auto") {
        Some(c) if CATEGORIES.contains(&c) => Some(c.to_string()),
        Some(c) => return Err(CoreError::invalid(format!("Unknown category '{c}'."))),
        None => None,
    };
    let n = conn.execute(
        "UPDATE files SET custom_category = ?1 WHERE id = ?2 AND game_id = ?3",
        params![category, file_id, game_id],
    )?;
    if n == 0 {
        return Err(CoreError::not_found("The file record no longer exists."));
    }
    Ok(())
}

/// Marks the main installer (or clears it with `None`).
pub fn set_primary(conn: &mut Connection, game_id: i64, file_id: Option<i64>) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute("UPDATE files SET is_primary = 0 WHERE game_id = ?1", [game_id])?;
    if let Some(fid) = file_id {
        let n = tx.execute("UPDATE files SET is_primary = 1 WHERE id = ?1 AND game_id = ?2", params![fid, game_id])?;
        if n == 0 {
            return Err(CoreError::not_found("The file record no longer exists."));
        }
    }
    tx.commit()?;
    Ok(())
}

/// Picks an obvious main installer: GOG-style `setup_*.exe` at the top level,
/// or the only top-level `.exe`/`.msi`. Returns the relative path.
pub fn guess_primary(files: &[FsFile]) -> Option<String> {
    let top: Vec<&FsFile> = files.iter().filter(|f| !f.rel_path.contains('/') && is_runnable(&f.name)).collect();
    let setups: Vec<&&FsFile> = top.iter().filter(|f| classify(&f.name) == "installer").collect();
    if let Some(best) = setups.iter().max_by_key(|f| f.size) {
        // GOG multi-part installers: `setup_x.exe` + `setup_x-1.bin`; the exe is the one to run.
        return Some(best.rel_path.clone());
    }
    if top.len() == 1 {
        return Some(top[0].rel_path.clone());
    }
    None
}

pub fn apply_guessed_primary(conn: &mut Connection, game_id: i64, rel: &str) -> Result<()> {
    let has_primary: bool =
        conn.query_row("SELECT EXISTS(SELECT 1 FROM files WHERE game_id = ?1 AND is_primary = 1)", [game_id], |r| r.get(0))?;
    if has_primary {
        return Ok(());
    }
    conn.execute("UPDATE files SET is_primary = 1 WHERE game_id = ?1 AND file_path = ?2", params![game_id, rel])?;
    Ok(())
}

/// Resolves a catalogued file for Open/Run, verifying that it really lies
/// inside the game folder (protects against `..` and symlink escapes).
pub fn resolve_file(conn: &Connection, layout: &ArchiveLayout, game_id: i64, file_id: i64) -> Result<(PathBuf, String)> {
    let folder = games::game_folder(conn, layout, game_id)?;
    let (rel, name): (String, String) = conn
        .query_row("SELECT file_path, filename FROM files WHERE id = ?1 AND game_id = ?2", params![file_id, game_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?
        .ok_or_else(|| CoreError::not_found("The file record no longer exists."))?;
    if !paths::is_safe_relative(&rel) {
        return Err(CoreError::PathOutsideAllowed);
    }
    if !folder.is_dir() {
        return Err(CoreError::invalid("The game folder is not available. Is the drive connected?"));
    }
    let full = folder.join(&rel);
    if !full.is_file() {
        return Err(CoreError::invalid(format!("The file \u{201c}{name}\u{201d} no longer exists.")));
    }
    Ok((paths::ensure_within(&folder, &full)?, name))
}

/// Resolves the game folder or one of its subfolders for "Open Folder".
pub fn resolve_subfolder(conn: &Connection, layout: &ArchiveLayout, game_id: i64, rel: Option<&str>) -> Result<PathBuf> {
    let folder = games::game_folder(conn, layout, game_id)?;
    if !folder.is_dir() {
        return Err(CoreError::invalid("The game folder is not available. Is the drive connected?"));
    }
    match rel.filter(|r| !r.is_empty()) {
        None => Ok(paths::simplify(&folder)),
        Some(r) => {
            if !paths::is_safe_relative(r) {
                return Err(CoreError::PathOutsideAllowed);
            }
            let full = folder.join(r);
            if !full.is_dir() {
                return Err(CoreError::invalid("The folder no longer exists."));
            }
            paths::ensure_within(&folder, &full)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert_eq!(classify("setup_disco_elysium_1.0.exe"), "installer");
        assert_eq!(classify("Game.exe"), "executable");
        assert_eq!(classify("data.msi"), "installer");
        assert_eq!(classify("setup-1.BIN"), "data");
        assert_eq!(classify("Manual.PDF"), "manual");
        assert_eq!(classify("ost.flac"), "music");
        assert_eq!(classify("extras.7z"), "archive");
        assert_eq!(classify("readme.md"), "text");
        assert_eq!(classify("cover.webp"), "image");
        assert_eq!(classify("weird.xyz"), "other");
        assert!(is_runnable("a.EXE") && is_runnable("b.msi") && !is_runnable("c.bin"));
        assert!(is_risky("run.BAT") && is_risky("x.lnk") && is_risky("a.exe") && !is_risky("m.pdf"));
    }

    #[test]
    fn primary_guess() {
        let f = |p: &str, s: u64| FsFile { rel_path: p.into(), name: p.rsplit('/').next().unwrap().into(), size: s };
        let files = vec![f("setup_game.exe", 10), f("setup_game-1.bin", 4000), f("extras/tool.exe", 1)];
        assert_eq!(guess_primary(&files).as_deref(), Some("setup_game.exe"));
        let files = vec![f("game.exe", 10), f("editor.exe", 4)];
        assert_eq!(guess_primary(&files), None);
    }
}
