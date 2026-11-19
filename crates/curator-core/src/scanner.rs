//! Archive Scanner: finds game candidates under a chosen folder.
//!
//! Detection rules (in this order):
//! 1. A folder that directly contains `.exe`, `.msi` or `.bin` files is a
//!    candidate; its subfolders (extras, soundtrack...) belong to it.
//! 2. When the installers sit in generic subfolders (`installer`, `extras`,
//!    `win`, ...), the parent folder becomes the candidate.
//! 3. A folder with only archives (`.zip`, `.7z`, `.rar`) and no candidate
//!    below it is a (weaker) candidate.
//!
//! The title comes from `goggame-*.info`, a `title.txt`-like text file, the
//! folder name, or the GOG installer name - in that order.
//!
//! The scanner only reads. It never executes, moves or modifies anything and
//! never imports without the user's confirmation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::archive::ArchiveLayout;
use crate::files::{self, FolderListing};
use crate::games::{self, ArtworkChange, GameInput};
use crate::{paths, text, CoreError, Result};

pub const DEFAULT_MAX_DEPTH: usize = 3;

const GENERIC_NAMES: &[&str] = &[
    "installer", "installers", "install", "setup", "setups", "extras", "extra", "bonus", "bonuses", "goodies",
    "soundtrack", "soundtracks", "ost", "music", "manual", "manuals", "docs", "documents", "patch", "patches",
    "update", "updates", "dlc", "dlcs", "artbook", "artwork", "wallpapers", "redist", "redistributables",
    "support", "__support", "win", "windows", "win32", "win64", "x64", "x86", "pc", "game", "files", "data",
    "bin", "linux", "mac", "osx", "offline", "gog", "drm-free",
];

fn is_generic(name: &str) -> bool {
    let lower = name.to_lowercase();
    GENERIC_NAMES.contains(&lower.trim())
        || !lower.chars().any(|c| c.is_alphabetic())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub dirs_scanned: usize,
    pub candidates: usize,
    pub current: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryCount {
    pub category: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub title: String,
    /// folder | gog_info | text_file | installer
    pub title_source: String,
    /// Absolute path on this computer.
    pub folder_path: String,
    /// Path as it would be stored (relative to the archive when possible).
    pub stored_path: String,
    pub file_count: usize,
    pub total_size: u64,
    pub categories: Vec<CategoryCount>,
    pub sample_files: Vec<String>,
    pub main_installer: Option<String>,
    /// new | in_catalog | possible_duplicate
    pub status: String,
    pub existing_game_id: Option<i64>,
    pub existing_title: Option<String>,
    /// "strong" (installers found) or "weak" (archives only)
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub root: String,
    pub candidates: Vec<Candidate>,
    pub cancelled: bool,
    pub dirs_scanned: usize,
    pub dirs_skipped: usize,
    pub elapsed_ms: u128,
}

struct Raw {
    path: PathBuf,
    strong: bool,
}

struct Ctx<'a> {
    max_depth: usize,
    cancel: &'a AtomicBool,
    progress: &'a mut dyn FnMut(&ScanProgress),
    dirs_scanned: usize,
    dirs_skipped: usize,
    found: usize,
    last_report: Instant,
}

impl Ctx<'_> {
    fn tick(&mut self, current: &Path) {
        self.dirs_scanned += 1;
        if self.last_report.elapsed() >= Duration::from_millis(100) {
            self.last_report = Instant::now();
            (self.progress)(&ScanProgress {
                dirs_scanned: self.dirs_scanned,
                candidates: self.found,
                current: paths::display(current),
            });
        }
    }
}

fn scan_dir(path: &Path, depth: usize, ctx: &mut Ctx) -> Result<Vec<Raw>> {
    if ctx.cancel.load(Ordering::Relaxed) {
        return Err(CoreError::Cancelled);
    }
    ctx.tick(path);
    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(e) if depth == 0 => return Err(e.into()),
        Err(_) => {
            ctx.dirs_skipped += 1;
            return Ok(Vec::new());
        }
    };
    let mut strong = false;
    let mut has_archive = false;
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if ft.is_dir() {
            if !name.starts_with('.') && !name.starts_with('$') && !name.eq_ignore_ascii_case("System Volume Information") {
                subdirs.push(e.path());
            }
        } else if ft.is_file() {
            match files::extension(&name).as_str() {
                "exe" | "msi" | "bin" => strong = true,
                "zip" | "7z" | "rar" => has_archive = true,
                _ => {}
            }
        }
    }
    if strong {
        ctx.found += 1;
        return Ok(vec![Raw { path: path.to_path_buf(), strong: true }]);
    }
    let mut found: Vec<Raw> = Vec::new();
    if depth < ctx.max_depth {
        subdirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        for sub in subdirs {
            found.extend(scan_dir(&sub, depth + 1, ctx)?);
        }
    }
    // Installers hidden in generic subfolders -> this folder is the game.
    let all_generic_children = !found.is_empty()
        && found.iter().all(|c| {
            c.path.parent() == Some(path)
                && c.path.file_name().map(|n| is_generic(&n.to_string_lossy())).unwrap_or(false)
        });
    if all_generic_children {
        let strong = found.iter().any(|c| c.strong);
        ctx.found = ctx.found + 1 - found.len();
        return Ok(vec![Raw { path: path.to_path_buf(), strong }]);
    }
    if found.is_empty() && has_archive && depth > 0 {
        ctx.found += 1;
        return Ok(vec![Raw { path: path.to_path_buf(), strong: false }]);
    }
    Ok(found)
}

fn read_small_text(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > 4096 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.chars().take(120).collect())
}

/// Determines the candidate title. Returns `(title, source)`.
pub fn detect_title(folder: &Path, listing: &FolderListing) -> (String, String) {
    let top: Vec<&files::FsFile> = listing.files.iter().filter(|f| !f.rel_path.contains('/')).collect();

    // 1. GOG metadata file
    for f in &top {
        let lower = f.name.to_lowercase();
        if lower.starts_with("goggame-") && lower.ends_with(".info") && f.size < 64 * 1024 {
            if let Ok(s) = fs::read_to_string(folder.join(&f.rel_path)) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                    if let Some(name) = v.get("name").and_then(|n| n.as_str()).map(text::tidy).filter(|n| !n.is_empty()) {
                        return (name, "gog_info".into());
                    }
                }
            }
        }
    }
    // 2. Text file containing the title
    for wanted in ["title.txt", "game.txt", "name.txt", "gametitle.txt", "game_title.txt", "gameinfo.txt"] {
        if let Some(f) = top.iter().find(|f| f.name.eq_ignore_ascii_case(wanted)) {
            if let Some(line) = read_small_text(&folder.join(&f.rel_path)) {
                let line = line.strip_prefix("Title:").or_else(|| line.strip_prefix("title:")).unwrap_or(&line).trim().to_string();
                if !line.is_empty() {
                    return (line, "text_file".into());
                }
            }
        }
    }
    // 3. Folder name (unless meaningless), 4. installer name
    let folder_name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !folder_name.is_empty() && !is_generic(&folder_name) {
        return (text::clean_title(&folder_name), "folder".into());
    }
    if let Some(inst) = top.iter().find(|f| f.name.to_lowercase().starts_with("setup_") && files::is_runnable(&f.name)) {
        let stem = Path::new(&inst.name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let t = text::clean_title(&stem);
        if !t.is_empty() {
            return (t, "installer".into());
        }
    }
    let fallback = if folder_name.is_empty() { "Unnamed game".to_string() } else { text::clean_title(&folder_name) };
    (fallback, "folder".into())
}

fn describe(raw: &Raw, root: &Path) -> Candidate {
    let listing = files::list_folder(&raw.path, 3, 5000).unwrap_or_default();
    let (title, title_source) = detect_title(&raw.path, &listing);
    let mut cats: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut total = 0u64;
    for f in &listing.files {
        *cats.entry(files::classify(&f.name)).or_default() += 1;
        total += f.size;
    }
    let mut sample: Vec<String> = listing.files.iter().filter(|f| !f.rel_path.contains('/')).map(|f| f.rel_path.clone()).collect();
    sample.extend(listing.dirs.iter().filter(|d| d.depth == 1).map(|d| format!("{}/", d.rel_path)));
    sample.truncate(8);
    Candidate {
        title,
        title_source,
        folder_path: paths::display(&raw.path),
        stored_path: paths::to_portable(root, &raw.path),
        file_count: listing.files.len(),
        total_size: total,
        categories: cats.into_iter().map(|(c, n)| CategoryCount { category: c.to_string(), count: n }).collect(),
        sample_files: sample,
        main_installer: files::guess_primary(&listing.files),
        status: "new".into(),
        existing_game_id: None,
        existing_title: None,
        confidence: if raw.strong { "strong".into() } else { "weak".into() },
    }
}

/// Scans `scan_root`. `archive_root` is used to compute the stored form of
/// each path. Cancelling returns the candidates found so far.
pub fn scan(
    scan_root: &Path,
    archive_root: &Path,
    max_depth: usize,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&ScanProgress),
) -> Result<ScanResult> {
    let started = Instant::now();
    if !scan_root.is_dir() {
        return Err(CoreError::invalid(format!("The folder does not exist: {}", paths::display(scan_root))));
    }
    let mut ctx = Ctx {
        max_depth: max_depth.clamp(1, 8),
        cancel,
        progress,
        dirs_scanned: 0,
        dirs_skipped: 0,
        found: 0,
        last_report: Instant::now() - Duration::from_secs(1),
    };
    let (raw, cancelled) = match scan_dir(scan_root, 0, &mut ctx) {
        Ok(r) => (r, false),
        Err(CoreError::Cancelled) => (Vec::new(), true),
        Err(e) => return Err(e),
    };
    let mut candidates = Vec::with_capacity(raw.len());
    for r in &raw {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        candidates.push(describe(r, archive_root));
    }
    let cancelled = cancelled || cancel.load(Ordering::Relaxed);
    (ctx.progress)(&ScanProgress { dirs_scanned: ctx.dirs_scanned, candidates: candidates.len(), current: String::new() });
    Ok(ScanResult {
        root: paths::display(scan_root),
        candidates,
        cancelled,
        dirs_scanned: ctx.dirs_scanned,
        dirs_skipped: ctx.dirs_skipped,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

/// Marks candidates already present in the catalog (same folder, or a folder
/// inside an existing game's folder) and possible duplicates (same title).
pub fn annotate(conn: &Connection, candidates: &mut [Candidate]) -> Result<()> {
    let mut stmt = conn.prepare("SELECT id, title, title_search, folder_key FROM games")?;
    let existing: Vec<(i64, String, String, String)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    for c in candidates.iter_mut() {
        let key = paths::folder_key(&c.stored_path);
        let search = text::search_key(&c.title);
        let inside = existing.iter().find(|(_, _, _, k)| *k == key || key.starts_with(&format!("{k}/")));
        if let Some((id, title, _, _)) = inside {
            c.status = "in_catalog".into();
            c.existing_game_id = Some(*id);
            c.existing_title = Some(title.clone());
            continue;
        }
        let dup = existing
            .iter()
            .find(|(_, _, s, k)| *s == search || k.starts_with(&format!("{key}/")));
        if let Some((id, title, _, _)) = dup {
            c.status = "possible_duplicate".into();
            c.existing_game_id = Some(*id);
            c.existing_title = Some(title.clone());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportItem {
    pub folder_path: String,
    pub title: String,
    pub developer: String,
    pub release_year: Option<i64>,
    pub genre: String,
    pub artwork_path: Option<String>,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedGame {
    pub id: i64,
    pub title: String,
    /// The folder path exactly as it was sent in the import item.
    pub folder_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedItem {
    pub title: String,
    pub folder_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: Vec<ImportedGame>,
    pub skipped: Vec<SkippedItem>,
}

/// Imports the candidates the user selected. Duplicates (same folder) are
/// skipped with a reason; each game is committed in its own transaction so
/// one bad item does not undo the others.
pub fn import(conn: &mut Connection, layout: &ArchiveLayout, items: &[ImportItem]) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    for item in items {
        let artwork = match (&item.artwork_path, &item.thumbnail_path) {
            (Some(a), Some(t)) => Some(ArtworkChange::Set { artwork_path: a.clone(), thumbnail_path: t.clone(), crop: None }),
            _ => None,
        };
        let input = GameInput {
            title: item.title.clone(),
            folder_path: item.folder_path.clone(),
            developer: item.developer.clone(),
            release_year: item.release_year,
            genre: item.genre.clone(),
            artwork,
            ..Default::default()
        };
        match games::create_game(conn, layout, &input) {
            Ok(id) => {
                let folder = paths::resolve(&layout.root, &item.folder_path);
                if let Ok(listing) = files::list_folder(&folder, files::MAX_DEPTH, files::MAX_ENTRIES) {
                    files::sync_files(conn, id, &listing)?;
                    if let Some(p) = files::guess_primary(&listing.files) {
                        files::apply_guessed_primary(conn, id, &p)?;
                    }
                }
                report.imported.push(ImportedGame { id, title: item.title.trim().to_string(), folder_path: item.folder_path.clone() });
            }
            Err(CoreError::Invalid(reason)) | Err(CoreError::NotFound(reason)) => {
                report.skipped.push(SkippedItem { title: item.title.clone(), folder_path: item.folder_path.clone(), reason });
            }
            Err(e) => return Err(e),
        }
    }
    Ok(report)
}
