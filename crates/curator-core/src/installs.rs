//! Installed games on this computer.
//!
//! The archive keeps installers; the games themselves are installed on each
//! PC separately. The application layer reads the list of installed programs
//! (read-only: the GOG and Windows "Uninstall" registry keys) and this module
//! matches it to the catalog. Matching is deliberately strict (same name
//! after normalisation, or same name as the installer file), so that a game is
//! never shown as installed by mistake; the user can always point to the
//! game's program manually.
//!
//! An installation belongs to one computer - the archive drive travels - so
//! every record carries the computer name. Detected records are refreshed at
//! each start; records set by the user are changed only by the user.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{db, paths, text, CoreError, Result};

/// One installed program as reported by the platform layer.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstalledProgram {
    pub name: String,
    /// Absolute folder of the installation.
    pub install_dir: String,
    /// Absolute path of the main program, when known.
    pub exe: String,
    pub arguments: String,
    /// Absolute working folder, when known.
    pub working_dir: String,
    /// "gog" (GOG registry key) or "uninstall" (Windows list of programs).
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStatus {
    pub machine: String,
    pub install_dir: String,
    /// Absolute path of the program that Play starts ("" = unknown).
    pub exe: String,
    /// "gog" | "uninstall" | "manual"
    pub source: String,
    /// The installation folder exists on this computer.
    pub found: bool,
    pub can_play: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectReport {
    pub machine: String,
    /// Games found installed automatically.
    pub detected: usize,
    /// Games whose location the user set.
    pub manual: usize,
    /// Installed programs looked at.
    pub programs: usize,
}

/// What Play starts. Paths are verified to exist and to lie inside the
/// installation folder.
#[derive(Debug, Clone)]
pub struct LaunchTarget {
    pub exe: PathBuf,
    pub arguments: String,
    pub working_dir: PathBuf,
}

/// Normalises the computer name used as the key of installations.
pub fn machine_key(raw: &str) -> String {
    let m: String = raw.trim().chars().take(64).collect::<String>().to_uppercase();
    if m.is_empty() {
        "THIS-PC".into()
    } else {
        m
    }
}

// ---------------------------------------------------------------- matching

/// Name used for matching: lower case, no diacritics, no trademark signs or
/// apostrophes, "&" as "and", punctuation as spaces, without a leading "the".
pub fn match_key(name: &str) -> String {
    let folded = text::search_key(name);
    let mut out = String::with_capacity(folded.len());
    for c in folded.chars() {
        match c {
            '\'' | '\u{2019}' | '\u{2122}' | '\u{ae}' | '\u{a9}' => {}
            '&' => out.push_str(" and "),
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    let words: Vec<&str> = out.split_whitespace().collect();
    let words = if words.len() > 1 && words[0] == "the" { &words[1..] } else { &words[..] };
    words.join(" ")
}

/// The title before an edition suffix after a dash ("X - Enhanced Edition").
fn edition_base(name: &str) -> Option<&str> {
    [" - ", " \u{2013} ", " \u{2014} "].iter().filter_map(|sep| name.find(sep)).min().map(|i| name[..i].trim()).filter(|b| !b.is_empty())
}

/// "setup_system_shock_2_2.4_(gog-3)_(12345).exe" -> "system shock 2".
pub fn installer_slug(filename: &str) -> Option<String> {
    let stem = filename.rsplit_once('.').map(|(s, _)| s).unwrap_or(filename).to_lowercase();
    let rest = stem.strip_prefix("setup_")?;
    let mut words = Vec::new();
    for tok in rest.split('_') {
        let version = tok.starts_with('(')
            || (tok.contains('.') && tok.chars().any(|c| c.is_ascii_digit()))
            || (tok.len() > 1 && tok.starts_with('v') && tok[1..].chars().all(|c| c.is_ascii_digit() || c == '.'));
        if version {
            break;
        }
        if !tok.is_empty() {
            words.push(tok);
        }
    }
    let slug = match_key(&words.join(" "));
    (slug.len() >= 3).then_some(slug)
}

/// The primary program of a GOG installation, from its `goggame-*.info`.
#[derive(Debug, Clone, PartialEq)]
pub struct GogPlayTask {
    pub exe_rel: String,
    pub arguments: String,
    pub working_dir_rel: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct GogInfo {
    game_id: String,
    root_game_id: String,
    play_tasks: Vec<GogTask>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct GogTask {
    #[serde(rename = "type")]
    kind: String,
    is_primary: bool,
    category: String,
    path: String,
    arguments: String,
    working_dir: String,
}

pub fn read_gog_play_task(install_dir: &Path) -> Option<GogPlayTask> {
    let mut best: Option<(i32, GogPlayTask)> = None;
    for entry in fs::read_dir(install_dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !(name.starts_with("goggame-") && name.ends_with(".info")) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.len() > 256 * 1024 {
            continue;
        }
        let Ok(raw) = fs::read_to_string(entry.path()) else { continue };
        let Ok(info) = serde_json::from_str::<GogInfo>(raw.trim_start_matches('\u{feff}')) else { continue };
        let is_root = info.root_game_id.is_empty() || info.root_game_id == info.game_id;
        for t in info.play_tasks.iter().filter(|t| t.kind == "FileTask" && !t.path.is_empty()) {
            let exe_rel = t.path.replace('\\', "/");
            if !paths::is_safe_relative(&exe_rel) || !exe_rel.to_lowercase().ends_with(".exe") {
                continue;
            }
            let score = (t.is_primary as i32) * 4 + (is_root as i32) * 2 + (t.category.is_empty() || t.category == "game") as i32;
            let task = GogPlayTask {
                exe_rel,
                arguments: t.arguments.clone(),
                working_dir_rel: t.working_dir.replace('\\', "/"),
            };
            if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
                best = Some((score, task));
            }
        }
    }
    best.map(|(_, t)| t)
}

/// `target` relative to `dir` when it lies inside it (Windows rules).
fn inside(dir: &Path, target: &Path) -> Option<String> {
    let rel = paths::to_slash(&paths::relative_to(dir, target)?);
    paths::is_safe_relative(&rel).then_some(rel)
}

fn is_program(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or("").to_lowercase();
    name.ends_with(".exe") && !name.starts_with("unins")
}

/// The record to store for a detected program, or None when its folder is
/// not on this computer.
struct Resolved {
    install_dir: String,
    exe_rel: String,
    arguments: String,
    working_dir_rel: String,
    source: &'static str,
}

fn resolve_program(p: &InstalledProgram) -> Option<Resolved> {
    let dir = PathBuf::from(p.install_dir.trim().trim_matches('"'));
    if p.install_dir.trim().is_empty() || !dir.is_absolute() || !dir.is_dir() {
        return None;
    }
    let source = if p.source == "gog" { "gog" } else { "uninstall" };
    let (mut exe_rel, mut arguments, mut working_dir_rel) = (String::new(), String::new(), String::new());
    if let Some(task) = read_gog_play_task(&dir).filter(|t| dir.join(&t.exe_rel).is_file()) {
        (exe_rel, arguments, working_dir_rel) = (task.exe_rel, task.arguments, task.working_dir_rel);
    } else if !p.exe.trim().is_empty() {
        let exe = PathBuf::from(p.exe.trim().trim_matches('"'));
        if let Some(rel) = inside(&dir, &exe).filter(|r| is_program(r) && exe.is_file()) {
            exe_rel = rel;
            arguments = p.arguments.trim().to_string();
            if !p.working_dir.trim().is_empty() {
                working_dir_rel = inside(&dir, Path::new(p.working_dir.trim())).unwrap_or_default();
            }
        }
    }
    Some(Resolved {
        install_dir: paths::display(&dir),
        exe_rel,
        arguments,
        working_dir_rel,
        source,
    })
}

/// Matches installed programs to games and stores the result for this
/// computer. Records the user set are kept as they are.
pub fn refresh(conn: &mut Connection, machine: &str, programs: &[InstalledProgram]) -> Result<DetectReport> {
    let machine = machine_key(machine);
    // keys of every game: its title and the name of its installer
    let mut by_key: HashMap<String, HashSet<i64>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT g.id, g.title,
                    (SELECT f.filename FROM files f WHERE f.game_id = g.id AND f.file_type = 'installer'
                      ORDER BY f.is_primary DESC, f.filename LIMIT 1)
             FROM games g",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?;
        for row in rows {
            let (id, title, installer) = row?;
            let mut keys = vec![match_key(&title)];
            keys.extend(installer.as_deref().and_then(installer_slug));
            for k in keys.into_iter().filter(|k| k.len() >= 3) {
                by_key.entry(k).or_default().insert(id);
            }
        }
    }

    // program -> exactly one game; a game with several programs prefers GOG
    let mut found: HashMap<i64, Resolved> = HashMap::new();
    for p in programs {
        // the program's own name; the folder name only when there is no name
        // (a folder "Gothic" may well hold "Gothic II")
        let name = if p.name.trim().is_empty() {
            Path::new(p.install_dir.trim()).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
        } else {
            p.name.clone()
        };
        let mut games: HashSet<i64> = by_key.get(&match_key(&name)).cloned().unwrap_or_default();
        if games.is_empty() {
            // an edition after a dash: "Disco Elysium - The Final Cut" -> "Disco Elysium"
            // (a colon usually starts another game's title, so it is not used)
            if let Some(base) = edition_base(&name) {
                games = by_key.get(&match_key(base)).cloned().unwrap_or_default();
            }
        }
        if games.len() != 1 {
            continue; // unknown, or ambiguous: never guess
        }
        let id = *games.iter().next().expect("one game");
        let Some(res) = resolve_program(p) else { continue };
        let better = match found.get(&id) {
            None => true,
            Some(cur) => (res.source == "gog" && cur.source != "gog") || (cur.exe_rel.is_empty() && !res.exe_rel.is_empty()),
        };
        if better {
            found.insert(id, res);
        }
    }

    let tx = conn.transaction()?;
    let now = db::now_iso(&tx)?;
    let manual: HashSet<i64> = {
        let mut stmt = tx.prepare("SELECT game_id FROM installations WHERE machine = ?1 AND source = 'manual'")?;
        let ids = stmt.query_map([&machine], |r| r.get(0))?.collect::<rusqlite::Result<HashSet<i64>>>()?;
        ids
    };
    let previous: Vec<i64> = {
        let mut stmt = tx.prepare("SELECT game_id FROM installations WHERE machine = ?1 AND source <> 'manual'")?;
        let ids = stmt.query_map([&machine], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        ids
    };
    for id in previous.iter().filter(|id| !found.contains_key(id)) {
        tx.execute("DELETE FROM installations WHERE game_id = ?1 AND machine = ?2 AND source <> 'manual'", params![id, machine])?;
    }
    let mut detected = 0;
    for (id, r) in &found {
        if manual.contains(id) {
            continue;
        }
        tx.execute(
            "INSERT INTO installations (game_id, machine, install_dir, exe_path, arguments, working_dir, source, date_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (game_id, machine) DO UPDATE SET install_dir = excluded.install_dir, exe_path = excluded.exe_path,
                 arguments = excluded.arguments, working_dir = excluded.working_dir, source = excluded.source,
                 date_updated = excluded.date_updated",
            params![id, machine, r.install_dir, r.exe_rel, r.arguments, r.working_dir_rel, r.source, now],
        )?;
        detected += 1;
    }
    tx.commit()?;
    Ok(DetectReport { machine, detected, manual: manual.len(), programs: programs.len() })
}

// ---------------------------------------------------------------- one game

type Row = (String, String, String, String, String);

fn row(conn: &Connection, machine: &str, game_id: i64) -> Result<Option<Row>> {
    Ok(conn
        .query_row(
            "SELECT install_dir, exe_path, arguments, working_dir, source FROM installations WHERE game_id = ?1 AND machine = ?2",
            params![game_id, machine_key(machine)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?)
}

pub fn status(conn: &Connection, machine: &str, game_id: i64) -> Result<Option<InstallStatus>> {
    let Some((dir, exe_rel, _, _, source)) = row(conn, machine, game_id)? else { return Ok(None) };
    let dir_p = PathBuf::from(&dir);
    let found = dir_p.is_dir();
    let exe = if exe_rel.is_empty() { String::new() } else { paths::display(&dir_p.join(&exe_rel)) };
    let can_play = found && !exe_rel.is_empty() && dir_p.join(&exe_rel).is_file();
    Ok(Some(InstallStatus { machine: machine_key(machine), install_dir: dir, exe, source, found, can_play }))
}

/// The user points to the game's program (.exe). The installation folder is
/// the nearest folder with a `goggame-*.info` (up to three levels up), or the
/// program's own folder.
pub fn set_manual(conn: &Connection, machine: &str, game_id: i64, exe: &Path) -> Result<InstallStatus> {
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM games WHERE id = ?1)", [game_id], |r| r.get(0))?;
    if !exists {
        return Err(CoreError::not_found("The game no longer exists."));
    }
    let is_exe = exe.extension().map(|e| e.eq_ignore_ascii_case("exe")).unwrap_or(false);
    if !exe.is_absolute() || !is_exe || !exe.is_file() {
        return Err(CoreError::invalid("Please choose the game's program (an .exe file)."));
    }
    let exe_dir = exe.parent().ok_or_else(|| CoreError::invalid("Please choose the game's program (an .exe file)."))?;
    let mut install_dir = exe_dir.to_path_buf();
    let mut task = None;
    for dir in exe_dir.ancestors().take(4) {
        if let Some(t) = read_gog_play_task(dir) {
            install_dir = dir.to_path_buf();
            task = Some(t);
            break;
        }
    }
    let exe_rel = inside(&install_dir, exe).ok_or(CoreError::PathOutsideAllowed)?;
    let (arguments, working_dir) = match task {
        Some(t) if t.exe_rel.eq_ignore_ascii_case(&exe_rel) => (t.arguments, t.working_dir_rel),
        _ => (String::new(), String::new()),
    };
    let now = db::now_iso(conn)?;
    conn.execute(
        "INSERT INTO installations (game_id, machine, install_dir, exe_path, arguments, working_dir, source, date_updated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'manual', ?7)
         ON CONFLICT (game_id, machine) DO UPDATE SET install_dir = excluded.install_dir, exe_path = excluded.exe_path,
             arguments = excluded.arguments, working_dir = excluded.working_dir, source = 'manual',
             date_updated = excluded.date_updated",
        params![game_id, machine_key(machine), paths::display(&install_dir), exe_rel, arguments, working_dir, now],
    )?;
    status(conn, machine, game_id)?.ok_or_else(|| CoreError::not_found("The game no longer exists."))
}

/// Forgets the installation of a game on this computer (only the record;
/// nothing on disk is touched). The next detection may find it again.
pub fn forget(conn: &Connection, machine: &str, game_id: i64) -> Result<()> {
    conn.execute("DELETE FROM installations WHERE game_id = ?1 AND machine = ?2", params![game_id, machine_key(machine)])?;
    Ok(())
}

/// The installation folder, verified to exist.
pub fn install_dir(conn: &Connection, machine: &str, game_id: i64) -> Result<PathBuf> {
    let (dir, ..) = row(conn, machine, game_id)?.ok_or_else(|| CoreError::not_found("The game is not installed on this computer."))?;
    let dir = PathBuf::from(dir);
    if !dir.is_dir() {
        return Err(CoreError::not_found(format!("The installation folder was not found:\n{}", paths::display(&dir))));
    }
    Ok(dir)
}

/// What Play starts. The program must exist and lie inside the installation
/// folder (also after following links).
pub fn launch_target(conn: &Connection, machine: &str, game_id: i64) -> Result<LaunchTarget> {
    let dir = install_dir(conn, machine, game_id)?;
    let (_, exe_rel, arguments, working_rel, _) = row(conn, machine, game_id)?.expect("checked above");
    if exe_rel.is_empty() || !paths::is_safe_relative(&exe_rel) || !is_program(&exe_rel) {
        return Err(CoreError::invalid("The game's program is not known. Use \u{201c}Locate\u{2026}\u{201d} to choose it."));
    }
    let exe = dir.join(&exe_rel);
    if !exe.is_file() {
        return Err(CoreError::not_found(format!("The game's program was not found:\n{}", paths::display(&exe))));
    }
    let exe = paths::ensure_within(&dir, &exe)?;
    let working_dir = if !working_rel.is_empty() && paths::is_safe_relative(&working_rel) && dir.join(&working_rel).is_dir() {
        paths::ensure_within(&dir, &dir.join(&working_rel))?
    } else {
        exe.parent().map(Path::to_path_buf).unwrap_or_else(|| dir.clone())
    };
    Ok(LaunchTarget { exe, arguments, working_dir })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_keys() {
        assert_eq!(match_key("Baldur's Gate 3"), "baldurs gate 3");
        assert_eq!(match_key("The Witcher\u{2122} 3: Wild Hunt"), "witcher 3 wild hunt");
        assert_eq!(match_key("Zaklínač 3"), "zaklinac 3");
        assert_eq!(match_key("Heroes of Might & Magic 3"), "heroes of might and magic 3");
    }

    #[test]
    fn editions() {
        assert_eq!(edition_base("Disco Elysium - The Final Cut"), Some("Disco Elysium"));
        assert_eq!(edition_base("Baldur\u{2019}s Gate \u{2013} Enhanced Edition"), Some("Baldur\u{2019}s Gate"));
        assert_eq!(edition_base("Fallout: New Vegas"), None);
        assert_eq!(edition_base("Gothic II"), None);
    }

    #[test]
    fn installer_slugs() {
        assert_eq!(installer_slug("setup_system_shock_2_2.4_(gog-3)_(12345).exe").as_deref(), Some("system shock 2"));
        assert_eq!(installer_slug("setup_baldurs_gate_3_v4.1.1.exe").as_deref(), Some("baldurs gate 3"));
        assert_eq!(installer_slug("setup_gothic_2.6.0.0_(28157).exe").as_deref(), Some("gothic"));
        assert_eq!(installer_slug("setup_x.exe"), None);
        assert_eq!(installer_slug("manual.pdf"), None);
    }
}
