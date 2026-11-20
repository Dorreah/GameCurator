//! Noticing when a game started with ▶ Play has ended, so Game Curator can
//! offer a journal entry while the impressions are fresh.
//!
//! Only games started from Game Curator are watched, and only while
//! Game Curator runs: one thread per started game waits for the program
//! Windows started, then - because many games start through a launcher that
//! quits at once - keeps looking (4 times a second) whether any program from
//! the game's installation folder is still running. The end is reported
//! within about a second; the offer itself is a small window that stays on
//! top (see `offer`). Nothing is written and no play time is stored or shown:
//! Game Curator is an archive, not a counter.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{lock, AppState};

/// The program Windows started for the game.
pub enum GameProcess {
    #[cfg(windows)]
    Handle(WinHandle),
    #[allow(dead_code)]
    Child(std::process::Child),
}

/// A process handle from ShellExecuteExW, closed when dropped.
#[cfg(windows)]
pub struct WinHandle(pub isize);

#[cfg(windows)]
impl Drop for WinHandle {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0 as _) };
    }
}

impl GameProcess {
    fn wait(self) {
        match self {
            #[cfg(windows)]
            GameProcess::Handle(h) => unsafe {
                use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};
                WaitForSingleObject(h.0 as _, INFINITE);
            },
            GameProcess::Child(mut c) => {
                let _ = c.wait();
            }
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GameEvent {
    pub game_id: i64,
    pub title: String,
}

/// Timing of the watch.
#[derive(Clone, Copy)]
pub struct Timing {
    /// How often the installation folder's programs are looked at.
    pub poll: Duration,
    /// No program from the folder for this long = the game has ended (a game
    /// that restarts itself, e.g. after changing the resolution, is not
    /// mistaken for its end).
    pub confirm: Duration,
    /// A started program that quit within `launcher` without anything else
    /// from the folder running may be a launcher handing over to the game:
    /// wait up to `handoff` for the game to appear.
    pub launcher: Duration,
    pub handoff: Duration,
    /// A shorter session (a crash at start, a launcher closed right away)
    /// does not ask for a journal entry.
    pub min_session: Duration,
}

impl Timing {
    pub fn standard() -> Self {
        let t = Timing {
            poll: Duration::from_millis(250),
            confirm: Duration::from_millis(750),
            launcher: Duration::from_secs(20),
            handoff: Duration::from_secs(8),
            min_session: Duration::from_secs(15),
        };
        #[cfg(feature = "e2e")]
        if std::env::var_os("GC_E2E_FAST_SESSIONS").is_some() {
            // the real speed of noticing the end; only short test "games" count
            return Timing { handoff: Duration::from_secs(2), min_session: Duration::ZERO, ..t };
        }
        t
    }
}

/// Is a folder specific enough to tell a game's programs apart (not a drive
/// root such as `C:\` or a shared system folder)?
fn specific_folder(dir: &Path) -> bool {
    let names: Vec<String> = dir
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(n) => Some(n.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    let shared = ["windows", "program files", "program files (x86)", "programdata", "users", "usr", "bin", "tmp"];
    match names.as_slice() {
        [] => false,
        [one] => !shared.contains(&one.as_str()),
        _ => true,
    }
}

/// Counts the running programs whose file lies in a game's installation
/// folder (or below), not counting Game Curator itself.
pub struct FolderWatch {
    base: Option<String>,
    me: u32,
    /// Windows: program file per (process id, program name), so a process is
    /// opened only once and not on every look.
    #[cfg(windows)]
    known: std::collections::HashMap<(u32, String), Option<PathBuf>>,
}

fn norm(p: &Path) -> String {
    let s = curator_core::paths::simplify(p).to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

impl FolderWatch {
    pub fn new(dir: &Path) -> Self {
        let base = specific_folder(dir).then(|| norm(&std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())));
        FolderWatch {
            base,
            me: std::process::id(),
            #[cfg(windows)]
            known: Default::default(),
        }
    }

    fn inside(&self, exe: &Path) -> bool {
        let Some(base) = &self.base else { return false };
        let e = norm(exe);
        e.len() > base.len() && e.starts_with(base.as_str()) && e.as_bytes()[base.len()] == b'/'
    }

    pub fn running(&mut self) -> usize {
        if self.base.is_none() {
            return 0;
        }
        #[cfg(windows)]
        {
            let mut seen = std::collections::HashSet::new();
            let mut count = 0;
            for (pid, name) in process_list() {
                if pid == self.me {
                    continue;
                }
                let key = (pid, name);
                let exe = self.known.entry(key.clone()).or_insert_with(|| program_file(pid)).clone();
                if exe.as_deref().is_some_and(|e| self.inside(e)) {
                    count += 1;
                }
                seen.insert(key);
            }
            self.known.retain(|k, _| seen.contains(k));
            count
        }
        #[cfg(not(windows))]
        {
            running_programs().into_iter().filter(|(pid, exe)| *pid != self.me && self.inside(exe)).count()
        }
    }
}

/// Number of running programs from `dir` (one look).
#[cfg(test)]
pub fn programs_running_from(dir: &Path) -> usize {
    FolderWatch::new(dir).running()
}

/// (process id, program file) of the running programs this user may see.
#[cfg(target_os = "linux")]
fn running_programs() -> Vec<(u32, PathBuf)> {
    let Ok(entries) = std::fs::read_dir("/proc") else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            let exe = std::fs::read_link(e.path().join("exe")).ok()?;
            Some((pid, exe))
        })
        .collect()
}

/// (process id, program name) of all processes - one cheap system snapshot.
#[cfg(windows)]
fn process_list() -> Vec<(u32, String)> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        while ok {
            if entry.th32ProcessID != 0 {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                out.push((entry.th32ProcessID, String::from_utf16_lossy(&entry.szExeFile[..len])));
            }
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
    }
    out
}

/// Full path of a process's program file (works for elevated games too:
/// "limited information" may be read across integrity levels).
#[cfg(windows)]
fn program_file(pid: u32) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(h);
        ok.then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..len as usize])))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn running_programs() -> Vec<(u32, PathBuf)> {
    Vec::new()
}

/// Waits until the game has ended: first the started program, then every
/// program from the installation folder (launchers hand over to the game).
/// Returns how long the game ran (until its last program was seen).
pub fn wait_for_end(process: Option<GameProcess>, install_dir: &Path, t: Timing) -> Duration {
    let start = Instant::now();
    let mut folder = FolderWatch::new(install_dir);
    let mut last_seen = start;
    if let Some(p) = process {
        p.wait();
        last_seen = Instant::now();
    }
    // a program from the folder has been seen after the started one: the game runs
    let mut handed_over = false;
    let quick_exit = last_seen.duration_since(start) < t.launcher;
    let mut quiet_since: Option<Instant> = None;
    loop {
        let now = Instant::now();
        if folder.running() > 0 {
            handed_over = true;
            last_seen = now;
            quiet_since = None;
        } else {
            let since = *quiet_since.get_or_insert(now);
            // right after a quick launcher, give the game a moment to appear
            let needed = if !handed_over && quick_exit { t.handoff } else { t.confirm };
            if now.duration_since(since) >= needed {
                break;
            }
        }
        std::thread::sleep(t.poll);
    }
    last_seen.duration_since(start)
}

/// Watches a game started with Play. Emits `gc-game-started` now,
/// `gc-game-stopped` when it has ended and - unless it ran only a few moments
/// - offers a journal entry in a small window on top of all others (see
/// `offer`). A game that is already being watched is not watched twice.
pub fn watch(app: &AppHandle, game_id: i64, title: String, install_dir: PathBuf, process: Option<GameProcess>) {
    {
        let state = app.state::<AppState>();
        let mut running = lock(&state.running_games);
        if !running.insert(game_id) {
            return;
        }
    }
    let event = GameEvent { game_id, title };
    let _ = app.emit("gc-game-started", event.clone());
    let handle = app.clone();
    let timing = Timing::standard();
    let spawned = std::thread::Builder::new().name(format!("game-session-{game_id}")).spawn(move || {
        let app = handle;
        let played = wait_for_end(process, &install_dir, timing);
        lock(&app.state::<AppState>().running_games).remove(&game_id);
        let _ = app.emit("gc-game-stopped", event.clone());
        if played >= timing.min_session {
            let _ = app.emit("gc-game-ended", event.clone());
            crate::offer::offer(&app, event);
        }
    });
    if spawned.is_err() {
        // without a thread nothing is watched; the game itself runs normally
        // (and is not left marked as running)
        lock(&app.state::<AppState>().running_games).remove(&game_id);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("gc-session-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fast() -> Timing {
        Timing {
            poll: Duration::from_millis(50),
            confirm: Duration::from_millis(200),
            launcher: Duration::from_secs(5),
            handoff: Duration::from_millis(600),
            min_session: Duration::ZERO,
        }
    }

    #[test]
    fn programs_from_an_installation_folder_are_seen_and_their_end_noticed() {
        let tmp = TempDir::new("inst");
        let inst = tmp.path().join("GOG Games").join("Game");
        std::fs::create_dir_all(&inst).unwrap();
        let game = inst.join("game-bin");
        std::fs::copy("/bin/sleep", &game).unwrap();
        assert_eq!(programs_running_from(&inst), 0);
        // a "launcher" quits at once and leaves the game running for a while
        let child = std::process::Command::new(&game).arg("1.2").spawn().unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(programs_running_from(&inst), 1);
        assert_eq!(programs_running_from(tmp.path().join("GOG Games").join("Other").as_path()), 0, "another folder");
        assert_eq!(programs_running_from(Path::new("/")), 0, "a drive root is never used");
        assert!(!specific_folder(Path::new("/usr")) && specific_folder(&inst));
        let launcher = std::process::Command::new("true").spawn().unwrap();
        let t0 = Instant::now();
        let ran = wait_for_end(Some(GameProcess::Child(launcher)), &inst, fast());
        assert!(ran >= Duration::from_millis(800), "waited for the game, not the launcher: {ran:?}");
        // noticed right away: within the confirmation time (+ one look) after the game ended
        let late = t0.elapsed().saturating_sub(Duration::from_millis(1050));
        assert!(late < Duration::from_millis(600), "end noticed {late:?} after it happened");
        assert_eq!(programs_running_from(&inst), 0);
        drop(child);
    }

    #[test]
    fn a_launcher_may_hand_over_to_a_game_that_starts_a_moment_later() {
        let tmp = TempDir::new("handoff");
        let inst = tmp.path().join("Games").join("Late");
        std::fs::create_dir_all(&inst).unwrap();
        let game = inst.join("late-bin");
        std::fs::copy("/bin/sleep", &game).unwrap();
        // the game appears 300 ms after the launcher has quit, then runs 0.5 s
        let _later = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("sleep 0.3; exec '{}' 0.5", game.display()))
            .spawn()
            .unwrap();
        let launcher = std::process::Command::new("true").spawn().unwrap();
        let ran = wait_for_end(Some(GameProcess::Child(launcher)), &inst, fast());
        assert!(ran >= Duration::from_millis(700), "the late game was waited for: {ran:?}");
        // nothing appears at all: the watch ends after the hand-over time
        let t0 = Instant::now();
        let ran = wait_for_end(Some(GameProcess::Child(std::process::Command::new("true").spawn().unwrap())), &inst, fast());
        assert!(ran < Duration::from_millis(200) && t0.elapsed() < Duration::from_millis(1200), "{ran:?} {:?}", t0.elapsed());
    }

    #[test]
    fn the_started_program_itself_is_waited_for() {
        let tmp = TempDir::new("own");
        let child = std::process::Command::new("sleep").arg("0.6").spawn().unwrap();
        let ran = wait_for_end(Some(GameProcess::Child(child)), tmp.path(), fast());
        assert!(ran >= Duration::from_millis(600), "{ran:?}");
    }
}
