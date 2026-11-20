//! Reading manuals and listening to soundtracks inside Game Curator.
//!
//! The application opens PDF files in its own viewer window and plays music
//! in a small player window. This module decides which files qualify,
//! builds the playlist of a game (its archive files and additional files)
//! and reads parts of a file for the viewer. It only ever reads: nothing
//! here writes, moves or renames a file.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};

use crate::archive::ArchiveLayout;
use crate::{additions, files, games, paths, CoreError, Result};

/// What Game Curator can show itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Pdf,
    Audio,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Pdf => "pdf",
            MediaKind::Audio => "audio",
        }
    }
}

/// Audio formats the player offers. WebView2 (Chromium) plays all of them;
/// a file it cannot decode is reported in the player, which also offers the
/// default application.
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "ogg", "oga", "opus", "flac", "wav", "m4a", "aac"];

pub fn kind_of(name: &str) -> Option<MediaKind> {
    let ext = files::extension(name);
    if ext == "pdf" {
        Some(MediaKind::Pdf)
    } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaKind::Audio)
    } else {
        None
    }
}

/// Largest piece the viewer may ask for at once.
pub const MAX_READ: u64 = 16 * 1024 * 1024;

/// Reads `len` bytes from `offset` (fewer at the end of the file).
pub fn read_range(path: &Path, offset: u64, len: u64) -> Result<Vec<u8>> {
    if len > MAX_READ {
        return Err(CoreError::invalid("Too much data requested at once."));
    }
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    if offset >= size || len == 0 {
        return Ok(Vec::new());
    }
    let len = len.min(size - offset);
    f.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len as usize];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// One track of a game's soundtrack.
#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
    /// Name shown in the player (the catalog name when the user gave one).
    pub name: String,
    pub size: u64,
    /// Where the track comes from: "archive" or "addition", with its id, so
    /// the clicked file can be found in the list.
    pub source: &'static str,
    pub id: i64,
}

fn shown_name(display: Option<&str>, filename: &str) -> String {
    display.map(str::to_string).unwrap_or_else(|| {
        let own = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
        // "01 - Main Theme.mp3" -> "01 - Main Theme"
        match own.rfind('.') {
            Some(dot) if dot > 0 => own[..dot].to_string(),
            _ => own.to_string(),
        }
    })
}

/// Compares names so that "Track 2" comes before "Track 10".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n.push(c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    m.push(d);
                    y.next();
                }
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let ord = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}

/// The music of a game: audio files in its archive folder (sorted by path,
/// so a "Soundtrack" folder plays in order), then audio additional files.
/// Files that are missing or lie outside their folder are left out.
pub fn game_tracks(conn: &Connection, layout: &ArchiveLayout, game_id: i64) -> Result<Vec<Track>> {
    let mut out = Vec::new();
    let folder = games::game_folder(conn, layout, game_id)?;
    if folder.is_dir() {
        let mut archive: Vec<_> = files::file_records(conn, game_id)?
            .into_iter()
            .filter(|f| !f.is_missing && kind_of(&f.filename) == Some(MediaKind::Audio) && paths::is_safe_relative(&f.file_path))
            .collect();
        archive.sort_by(|a, b| natural_cmp(&a.file_path, &b.file_path));
        for f in archive {
            let full = folder.join(&f.file_path);
            let Ok(full) = paths::ensure_within(&folder, &full) else { continue };
            let Ok(meta) = full.metadata() else { continue };
            if meta.is_file() {
                out.push(Track { name: shown_name(f.display_name.as_deref(), &f.filename), size: meta.len(), path: full, source: "archive", id: f.id });
            }
        }
    }
    let mut extra: Vec<_> = additions::list(conn, layout, game_id)?
        .into_iter()
        .filter(|a| a.exists && kind_of(&a.filename) == Some(MediaKind::Audio))
        .collect();
    extra.sort_by(|a, b| natural_cmp(&a.filename, &b.filename));
    for a in extra {
        let Ok(path) = additions::file(conn, layout, a.id) else { continue };
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(Track { name: shown_name(a.display_name.as_deref(), &a.filename), size, path, source: "addition", id: a.id });
    }
    Ok(out)
}

/// Title of a game, for window titles.
pub fn game_title(conn: &Connection, game_id: i64) -> Result<String> {
    conn.query_row("SELECT title FROM games WHERE id = ?1", [game_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| CoreError::not_found("The game no longer exists."))
}

/// Name shown for an archive file (its catalog name or the file name).
pub fn file_title(conn: &Connection, game_id: i64, file_id: i64) -> Result<String> {
    let (display, filename): (Option<String>, String) = conn
        .query_row("SELECT display_name, filename FROM files WHERE id = ?1 AND game_id = ?2", [file_id, game_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?
        .ok_or_else(|| CoreError::not_found("The file record no longer exists."))?;
    Ok(shown_name(display.as_deref(), &filename))
}

/// Name shown for an additional file.
pub fn addition_title(a: &additions::Addition) -> String {
    shown_name(a.display_name.as_deref(), &a.filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(kind_of("Manual.PDF"), Some(MediaKind::Pdf));
        assert_eq!(kind_of("Soundtrack/01 - Theme.mp3"), Some(MediaKind::Audio));
        assert_eq!(kind_of("track.flac"), Some(MediaKind::Audio));
        assert_eq!(kind_of("setup.exe"), None);
        assert_eq!(kind_of("readme"), None);
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["Track 10.mp3", "track 2.mp3", "Track 1.mp3", "Intro.mp3", "Track 02b.mp3"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["Intro.mp3", "Track 1.mp3", "track 2.mp3", "Track 02b.mp3", "Track 10.mp3"]);
    }

    #[test]
    fn names() {
        assert_eq!(shown_name(None, "Soundtrack/01 - Main Theme.mp3"), "01 - Main Theme");
        assert_eq!(shown_name(Some("Main theme"), "x.mp3"), "Main theme");
        assert_eq!(shown_name(None, ".hidden"), ".hidden");
    }

    #[test]
    fn ranges() {
        let dir = std::env::temp_dir().join(format!("gc-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.bin");
        std::fs::write(&p, (0u8..100).collect::<Vec<_>>()).unwrap();
        assert_eq!(read_range(&p, 10, 5).unwrap(), vec![10, 11, 12, 13, 14]);
        assert_eq!(read_range(&p, 95, 50).unwrap(), vec![95, 96, 97, 98, 99], "shortened at the end");
        assert!(read_range(&p, 100, 5).unwrap().is_empty());
        assert!(read_range(&p, 0, MAX_READ + 1).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
