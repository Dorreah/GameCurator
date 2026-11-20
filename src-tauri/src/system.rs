//! Windows operating-system interactions: archive location, WebView2
//! availability, error boxes, drive type, opening files/folders, starting
//! programs (ShellExecuteW) and the list of installed games (registry,
//! read-only - Game Curator never writes to the registry).
//!
//! Callers must validate paths (see `curator_core::files`) before calling
//! anything here. Non-Windows code paths exist only for the `e2e` test
//! harness build.

use std::path::{Path, PathBuf};

/// The archive root is the folder that contains `GameCurator.exe`.
/// `GAME_CURATOR_ROOT` overrides it (development and automated tests).
pub fn archive_root() -> Result<PathBuf, String> {
    if let Some(r) = std::env::var_os("GAME_CURATOR_ROOT").filter(|r| !r.is_empty()) {
        return Ok(PathBuf::from(r));
    }
    let exe = std::env::current_exe().map_err(|e| format!("Cannot determine the application folder: {e}"))?;
    let dir = exe.parent().ok_or("Cannot determine the application folder.")?;
    Ok(curator_core::paths::simplify(dir))
}

/// If a WebView2 "Fixed Version" runtime is shipped next to the exe (folder
/// `WebView2Runtime`), use it. This lets the archive run on computers without
/// an installed WebView2 Runtime. The system runtime is preferred.
pub fn prepare_webview_runtime(root: &Path) {
    #[cfg(windows)]
    {
        let fixed = root.join("WebView2Runtime");
        if fixed.join("msedgewebview2.exe").is_file() && std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER").is_none() {
            std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &fixed);
        }
    }
    #[cfg(not(windows))]
    let _ = root;
}

/// Returns the WebView2 version or a user-friendly explanation.
pub fn check_webview() -> Result<String, String> {
    tauri::webview_version().map_err(|_| {
        "Game Curator needs the Microsoft Edge WebView2 Runtime, which was not found on this computer.\n\n\
         Windows 11 and up-to-date Windows 10 include it. Otherwise install the \"Evergreen\" WebView2 Runtime \
         from Microsoft (search for \"WebView2 Runtime download\"), or copy a Fixed Version runtime into the \
         folder \"WebView2Runtime\" next to GameCurator.exe.\n\nNo data was changed."
            .to_string()
    })
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    pub fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn wide_path(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }
}

/// Shows a blocking error message before/without the main window.
pub fn fatal(title: &str, message: &str) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let t = win::wide(title);
        let m = win::wide(message);
        MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONERROR);
    }
    #[cfg(not(windows))]
    eprintln!("{title}: {message}");
}

/// Human-readable description of the drive the archive runs from.
pub fn drive_kind(root: &Path) -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
        let s = root.to_string_lossy();
        if s.len() >= 2 && s.as_bytes()[1] == b':' {
            let drive = format!("{}\\", &s[..2]);
            let kind = unsafe { GetDriveTypeW(win::wide(&drive).as_ptr()) };
            return match kind {
                2 => format!("removable drive {}", &s[..2]),
                3 => format!("fixed drive {} (external SSDs often report as fixed)", &s[..2]),
                4 => format!("network drive {}", &s[..2]),
                5 => format!("optical drive {}", &s[..2]),
                _ => format!("drive {}", &s[..2]),
            };
        }
        "network or unknown location".to_string()
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        "test harness (not Windows)".to_string()
    }
}

#[cfg(feature = "e2e")]
fn e2e_log(action: &str, path: &Path) -> Option<Result<(), String>> {
    use std::io::Write;
    let log = std::env::var_os("GC_E2E_ACTION_LOG")?;
    let result = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .and_then(|mut f| writeln!(f, "{action} {}", path.display()))
        .map_err(|e| e.to_string());
    Some(result)
}

/// ShellExecuteW "open" with optional parameters and working folder.
#[cfg(windows)]
fn shell_execute(path: &Path, params: &str, dir: Option<&Path>) -> Result<(), String> {
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let file = win::wide_path(path);
        let dir = dir.map(win::wide_path);
        let params = (!params.trim().is_empty()).then(|| win::wide(params.trim()));
        let verb = win::wide("open");
        let ret = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                params.as_ref().map(|p| p.as_ptr()).unwrap_or(std::ptr::null()),
                dir.as_ref().map(|d| d.as_ptr()).unwrap_or(std::ptr::null()),
                SW_SHOWNORMAL,
            )
        };
        let code = ret as isize;
        if code > 32 {
            Ok(())
        } else {
            Err(match code {
                2 | 3 => "The file was not found.".to_string(),
                5 => "Windows denied access (the action may have been cancelled).".to_string(),
                31 => "No application is associated with this file type.".to_string(),
                _ => format!("Windows could not open the file (code {code})."),
            })
        }
    }
}

/// Opens a file with its default application or a folder in Explorer. The
/// working directory is the file's folder, which old installers expect.
pub fn shell_open(path: &Path) -> Result<(), String> {
    #[cfg(feature = "e2e")]
    if let Some(r) = e2e_log("OPEN", path) {
        return r;
    }
    #[cfg(windows)]
    {
        shell_execute(path, "", path.parent().filter(|_| path.is_file()))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Opening files is only supported on Windows.".to_string())
    }
}

/// Starts an installed game (after the user confirmed it). Returns the
/// started program when Windows gives a handle to it, so the end of the game
/// can be noticed (`session`).
pub fn start_game(target: &curator_core::installs::LaunchTarget) -> Result<Option<crate::session::GameProcess>, String> {
    #[cfg(feature = "e2e")]
    if let Some(r) = e2e_log("PLAY", &target.exe) {
        r?;
        // test harness: a stand-in "game" (a launcher script) can be started
        if let Some(cmd) = std::env::var_os("GC_E2E_GAME_CMD") {
            let child = std::process::Command::new(cmd).current_dir(&target.working_dir).spawn().map_err(|e| e.to_string())?;
            return Ok(Some(crate::session::GameProcess::Child(child)));
        }
        return Ok(None);
    }
    #[cfg(windows)]
    {
        shell_execute_watch(&target.exe, &target.arguments, &target.working_dir)
            .map(|h| h.map(crate::session::GameProcess::Handle))
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err("Starting games is only supported on Windows.".to_string())
    }
}

/// ShellExecuteExW "open" that keeps a handle to the started process.
#[cfg(windows)]
fn shell_execute_watch(path: &Path, params: &str, dir: &Path) -> Result<Option<crate::session::WinHandle>, String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let file = win::wide_path(path);
    let dir = win::wide_path(dir);
    let params = (!params.trim().is_empty()).then(|| win::wide(params.trim()));
    let verb = win::wide("open");
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ref().map(|p| p.as_ptr()).unwrap_or(std::ptr::null());
    info.lpDirectory = dir.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let code = unsafe { GetLastError() };
        return Err(match code {
            2 | 3 => "The file was not found.".to_string(),
            5 => "Windows denied access (the action may have been cancelled).".to_string(),
            1155 => "No application is associated with this file type.".to_string(),
            1223 => "Starting the game was cancelled.".to_string(),
            _ => format!("Windows could not start the game (code {code})."),
        });
    }
    Ok((!info.hProcess.is_null()).then(|| crate::session::WinHandle(info.hProcess as isize)))
}

/// Name of this computer: the key of the installations stored in the archive.
pub fn machine_name() -> String {
    #[cfg(feature = "e2e")]
    if let Some(m) = std::env::var_os("GC_E2E_MACHINE") {
        return curator_core::installs::machine_key(&m.to_string_lossy());
    }
    let raw = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default();
    curator_core::installs::machine_key(&raw)
}

/// Installed programs this computer knows about: GOG games
/// (`HKLM\SOFTWARE\GOG.com\Games`) and the Windows list of installed
/// programs (`...\CurrentVersion\Uninstall`, machine and user). Read-only.
pub fn installed_programs() -> Vec<curator_core::installs::InstalledProgram> {
    #[cfg(feature = "e2e")]
    if let Some(f) = std::env::var_os("GC_E2E_INSTALLED") {
        return std::fs::read_to_string(f).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    }
    #[cfg(windows)]
    {
        reg::installed_programs()
    }
    #[cfg(not(windows))]
    Vec::new()
}

#[cfg(windows)]
mod reg {
    use curator_core::installs::InstalledProgram;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ,
        KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_SAM_FLAGS, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };

    use super::win::wide;

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn open(root: HKEY, path: &str, view: REG_SAM_FLAGS) -> Option<Key> {
        let mut h: HKEY = std::ptr::null_mut();
        let p = wide(path);
        let r = unsafe { RegOpenKeyExW(root, p.as_ptr(), 0, KEY_READ | view, &mut h) };
        (r == ERROR_SUCCESS).then_some(Key(h))
    }

    impl Key {
        fn subkeys(&self) -> Vec<String> {
            let mut out = Vec::new();
            for i in 0..20_000u32 {
                let mut buf = [0u16; 256];
                let mut len = buf.len() as u32;
                let r = unsafe {
                    RegEnumKeyExW(
                        self.0,
                        i,
                        buf.as_mut_ptr(),
                        &mut len,
                        std::ptr::null(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                if r != ERROR_SUCCESS {
                    break;
                }
                out.push(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]));
            }
            out
        }

        /// A string value of a subkey ("" when missing); REG_EXPAND_SZ is expanded.
        fn string(&self, sub: &str, name: &str) -> String {
            let (s, n) = (wide(sub), wide(name));
            let mut size: u32 = 0;
            let r = unsafe {
                RegGetValueW(self.0, s.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut size)
            };
            if r != ERROR_SUCCESS || size == 0 || size > 64 * 1024 {
                return String::new();
            }
            let mut buf = vec![0u16; size as usize / 2 + 2];
            let mut size = (buf.len() * 2) as u32;
            let r = unsafe {
                RegGetValueW(self.0, s.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
            };
            if r != ERROR_SUCCESS {
                return String::new();
            }
            let len = (size as usize / 2).min(buf.len());
            String::from_utf16_lossy(&buf[..len]).trim_end_matches('\0').trim().to_string()
        }

        fn dword(&self, sub: &str, name: &str) -> Option<u32> {
            let (s, n) = (wide(sub), wide(name));
            let mut v: u32 = 0;
            let mut size: u32 = 4;
            let r = unsafe {
                RegGetValueW(self.0, s.as_ptr(), n.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&mut v as *mut u32).cast(), &mut size)
            };
            (r == ERROR_SUCCESS).then_some(v)
        }
    }

    /// "\"C:\Games\X\x.exe\",0" -> C:\Games\X\x.exe
    fn icon_path(icon: &str) -> String {
        let s = icon.trim();
        let s = match s.rsplit_once(',') {
            Some((p, idx)) if idx.trim().trim_start_matches('-').chars().all(|c| c.is_ascii_digit()) => p,
            _ => s,
        };
        s.trim().trim_matches('"').to_string()
    }

    pub fn installed_programs() -> Vec<InstalledProgram> {
        let mut out = Vec::new();
        for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
            let Some(k) = open(HKEY_LOCAL_MACHINE, "SOFTWARE\\GOG.com\\Games", view) else { continue };
            for sub in k.subkeys() {
                out.push(InstalledProgram {
                    name: k.string(&sub, "gameName"),
                    install_dir: k.string(&sub, "path"),
                    exe: k.string(&sub, "exe"),
                    arguments: k.string(&sub, "launchParam"),
                    working_dir: k.string(&sub, "workingDir"),
                    source: "gog".into(),
                });
            }
        }
        let uninstall = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
        for (root, view) in [(HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY), (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY), (HKEY_CURRENT_USER, 0)] {
            let Some(k) = open(root, uninstall, view) else { continue };
            for sub in k.subkeys() {
                if k.dword(&sub, "SystemComponent") == Some(1) {
                    continue;
                }
                let name = k.string(&sub, "DisplayName");
                let dir = k.string(&sub, "InstallLocation");
                if name.is_empty() || dir.is_empty() {
                    continue;
                }
                out.push(InstalledProgram {
                    name,
                    install_dir: dir.trim_matches('"').to_string(),
                    exe: icon_path(&k.string(&sub, "DisplayIcon")),
                    source: "uninstall".into(),
                    ..Default::default()
                });
            }
        }
        out
    }
}

/// Starts a program after the user confirmed it. ShellExecute handles the
/// UAC prompt of installers and `.msi` packages.
pub fn run_program(path: &Path) -> Result<(), String> {
    #[cfg(feature = "e2e")]
    if let Some(r) = e2e_log("RUN", path) {
        return r;
    }
    shell_open(path)
}
