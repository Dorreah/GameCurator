//! Portable path handling.
//!
//! Every path stored in the database is either
//!  * **relative to the archive root** (the folder containing `GameCurator.exe`),
//!    written with forward slashes, e.g. `Games/System Shock 2` or
//!    `../GOG_Installers/Game_A`, or
//!  * **absolute**, only when the target is on a different drive than the archive
//!    and therefore cannot be expressed relatively.
//!
//! Relative paths make the archive independent of the drive letter: when the
//! whole drive becomes `E:` instead of `D:`, `../GOG_Installers/Game_A` still
//! resolves correctly.

use std::path::{Component, Path, PathBuf};

use crate::{CoreError, Result};

/// Removes `.` and resolves `..` purely lexically (no file-system access, so it
/// also works for folders on a disconnected drive).
pub fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                // Cannot go above the root of a drive.
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => out.push(comp),
            },
            c => out.push(c),
        }
    }
    let mut pb = PathBuf::new();
    for c in out {
        pb.push(c.as_os_str());
    }
    pb
}

/// Strips the Windows verbatim prefix (`\\?\C:\x` -> `C:\x`,
/// `\\?\UNC\srv\share` -> `\\srv\share`) produced by `canonicalize`.
pub fn simplify(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        let b = rest.as_bytes();
        if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

// Game Curator is a Windows application: path comparison and duplicate keys
// follow Windows rules (case-insensitive, `\` and `/` are equivalent) on
// every platform, so the test suite exercises exactly the Windows behaviour.

fn comp_eq(a: &Component, b: &Component) -> bool {
    a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
}

/// Computes `target` relative to `base`. Returns `None` when both are not on
/// the same drive / root (or are not absolute).
pub fn relative_to(base: &Path, target: &Path) -> Option<PathBuf> {
    let base = normalize_lexical(&simplify(base));
    let target = normalize_lexical(&simplify(target));
    if !base.is_absolute() || !target.is_absolute() {
        return None;
    }
    let b: Vec<Component> = base.components().collect();
    let t: Vec<Component> = target.components().collect();
    let anchor = b
        .iter()
        .take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
        .count();
    let mut common = 0;
    while common < b.len() && common < t.len() && comp_eq(&b[common], &t[common]) {
        common += 1;
    }
    if common < anchor {
        return None;
    }
    let mut rel = PathBuf::new();
    for _ in common..b.len() {
        rel.push("..");
    }
    for c in &t[common..] {
        rel.push(c.as_os_str());
    }
    Some(rel)
}

/// Converts a native path into the string written to the database.
pub fn to_slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Converts an absolute path into its portable, stored form.
pub fn to_portable(root: &Path, target: &Path) -> String {
    let target = simplify(target);
    match relative_to(root, &target) {
        Some(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Some(rel) => to_slash(&rel),
        None => to_slash(&normalize_lexical(&target)),
    }
}

/// Resolves a stored path against the current archive root.
pub fn resolve(root: &Path, stored: &str) -> PathBuf {
    let p = Path::new(stored);
    if p.is_absolute() {
        normalize_lexical(p)
    } else {
        normalize_lexical(&root.join(p))
    }
}

/// Whether the stored path is relative (i.e. survives drive-letter changes).
pub fn is_portable(stored: &str) -> bool {
    !Path::new(stored).is_absolute()
}

/// Key used for duplicate detection (Windows file systems are case-insensitive).
pub fn folder_key(stored: &str) -> String {
    let mut s = stored.replace('\\', "/").to_lowercase();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// True when `rel` is a harmless relative path: no `..`, no root, no drive.
pub fn is_safe_relative(rel: &str) -> bool {
    // ':' covers drive letters ("C:\x") and NTFS alternate data streams.
    if rel.is_empty() || rel.contains('\0') || rel.contains(':') {
        return false;
    }
    let p = Path::new(rel);
    if p.is_absolute() || rel.starts_with('/') || rel.starts_with('\\') {
        return false;
    }
    p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        && !rel.split(['/', '\\']).any(|seg| seg == "..")
}

/// Verifies (following symlinks) that `candidate` lies inside `base` and returns
/// the canonical, simplified path. Both must exist.
pub fn ensure_within(base: &Path, candidate: &Path) -> Result<PathBuf> {
    let base_c = std::fs::canonicalize(base)?;
    let cand_c = std::fs::canonicalize(candidate)?;
    if cand_c.starts_with(&base_c) {
        Ok(simplify(&cand_c))
    } else {
        Err(CoreError::PathOutsideAllowed)
    }
}

/// Path shown to the user (native separators, no verbatim prefix).
pub fn display(p: &Path) -> String {
    simplify(p).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_normalization() {
        assert_eq!(normalize_lexical(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
        assert_eq!(normalize_lexical(Path::new("/../x")), PathBuf::from("/x"));
        assert_eq!(normalize_lexical(Path::new("a/../../b")), PathBuf::from("../b"));
    }

    #[test]
    fn relative_inside_and_sibling() {
        let root = Path::new("/media/usb/GAME_CURATOR");
        assert_eq!(to_portable(root, Path::new("/media/usb/GAME_CURATOR/Games/Disco Elysium")), "Games/Disco Elysium");
        assert_eq!(to_portable(root, Path::new("/media/usb/GOG_Installers/Game_A")), "../GOG_Installers/Game_A");
        assert_eq!(to_portable(root, root), ".");
    }

    #[test]
    fn resolve_roundtrip_after_move() {
        let old_root = Path::new("/media/D/GAME_CURATOR");
        let stored = to_portable(old_root, Path::new("/media/D/GOG_Installers/Game_A"));
        // The drive is mounted elsewhere now (drive letter change equivalent).
        let new_root = Path::new("/media/E/GAME_CURATOR");
        assert_eq!(resolve(new_root, &stored), PathBuf::from("/media/E/GOG_Installers/Game_A"));
    }

    #[test]
    fn safe_relative() {
        assert!(is_safe_relative("Data/Thumbnails/a.webp"));
        assert!(!is_safe_relative("../secret"));
        assert!(!is_safe_relative("Data/../../x"));
        assert!(!is_safe_relative("/etc/passwd"));
        assert!(!is_safe_relative(""));
        assert!(!is_safe_relative("C:\\Windows"));
    }

    #[test]
    fn verbatim_prefix_is_stripped() {
        assert_eq!(simplify(Path::new(r"\\?\C:\Games")), PathBuf::from(r"C:\Games"));
        assert_eq!(simplify(Path::new(r"\\?\UNC\srv\share")), PathBuf::from(r"\\srv\share"));
    }

    #[test]
    fn keys_follow_windows_rules() {
        assert_eq!(folder_key("Games/X/"), folder_key("Games/X"));
        assert_eq!(folder_key("Games\\Disco Elysium"), folder_key("games/DISCO ELYSIUM"));
        // same drive folder written with different case is still relative
        assert_eq!(to_portable(Path::new("/media/USB/GAME_CURATOR"), Path::new("/media/usb/Gog/Game")), "../Gog/Game");
    }
}
