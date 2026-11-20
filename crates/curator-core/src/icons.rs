//! Icons of installed games, shown in the catalog view.
//!
//! Read-only: the icon is read from the installation - GOG's
//! `goggame-<id>.ico`, or the icon resource inside the game's program - and a
//! small copy is cached in `Data/Thumbnails/icon-*.webp`. Nothing in the
//! installation is ever changed, and the program is never run: only the
//! bytes of its icon resource are read, after parsing the PE headers with
//! strict bounds checks.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use image::{imageops::FilterType, DynamicImage, ImageFormat, RgbaImage};
use rusqlite::{params, Connection};

use crate::archive::ArchiveLayout;
use crate::{installs, paths, CoreError, Result};

/// Stored size; the catalog shows icons at 24 px, so they stay sharp on
/// screens with 200 % scaling.
pub const ICON_SIZE: u32 = 48;
/// Largest `.ico` file or icon image that is read.
const MAX_ICON_BYTES: usize = 4 << 20;
/// Largest piece of a program read in one go (headers, directories).
const MAX_READ: usize = 64 << 10;

// ---------------------------------------------------------------- sources

/// GOG puts `goggame-<id>.ico` into every installation folder.
pub fn find_gog_ico(install_dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(install_dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n.starts_with("goggame-") && n.ends_with(".ico") && e.path().is_file()
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found.into_iter().next()
}

fn read_at(f: &mut File, len: u64, off: u64, n: usize, max: usize) -> Option<Vec<u8>> {
    if n == 0 || n > max || off.checked_add(n as u64)? > len {
        return None;
    }
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut buf = vec![0u8; n];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

struct Section {
    va: u32,
    vsize: u32,
    raw_off: u32,
    raw_size: u32,
}

struct DirEntry {
    id: Option<u32>,
    is_dir: bool,
    offset: u32,
}

/// The icon images of a Windows program (`.exe`), best first: the images of
/// its first icon group (the one Explorer shows), largest and most colourful
/// first. Each image is returned as it is stored (PNG or a bitmap without
/// file header). `None` when the file is not a program or has no icon.
pub fn exe_icon_images(exe: &Path) -> Option<Vec<Vec<u8>>> {
    let mut f = File::open(exe).ok()?;
    let len = f.metadata().ok()?.len();
    let dos = read_at(&mut f, len, 0, 64, MAX_READ)?;
    if &dos[..2] != b"MZ" {
        return None;
    }
    let pe = u32_at(&dos, 0x3c)? as u64;
    let head = read_at(&mut f, len, pe, 24, MAX_READ)?;
    if &head[..4] != b"PE\0\0" {
        return None;
    }
    let n_sections = u16_at(&head, 6)? as usize;
    let opt_size = u16_at(&head, 20)? as usize;
    if n_sections == 0 || n_sections > 96 || opt_size < 96 {
        return None;
    }
    let opt = read_at(&mut f, len, pe + 24, opt_size, MAX_READ)?;
    let (count_at, dirs_at) = match u16_at(&opt, 0)? {
        0x10b => (92, 96),  // PE32
        0x20b => (108, 112), // PE32+
        _ => return None,
    };
    if u32_at(&opt, count_at)? < 3 {
        return None;
    }
    let rsrc_rva = u32_at(&opt, dirs_at + 16)?;
    if rsrc_rva == 0 {
        return None;
    }
    let table = read_at(&mut f, len, pe + 24 + opt_size as u64, n_sections * 40, MAX_READ)?;
    let sections: Vec<Section> = (0..n_sections)
        .filter_map(|i| {
            let b = &table[i * 40..i * 40 + 40];
            Some(Section { vsize: u32_at(b, 8)?, va: u32_at(b, 12)?, raw_size: u32_at(b, 16)?, raw_off: u32_at(b, 20)? })
        })
        .collect();
    let to_offset = |rva: u32| -> Option<u64> {
        sections.iter().find_map(|s| {
            let size = s.vsize.max(s.raw_size);
            let d = rva.checked_sub(s.va)?;
            (d < size && d < s.raw_size).then(|| s.raw_off as u64 + d as u64)
        })
    };
    let root = to_offset(rsrc_rva)?;
    let read_dir = |f: &mut File, rel: u32| -> Option<Vec<DirEntry>> {
        let h = read_at(f, len, root + rel as u64, 16, MAX_READ)?;
        let n = u16_at(&h, 12)? as usize + u16_at(&h, 14)? as usize;
        if n == 0 || n > 4096 {
            return None;
        }
        let raw = read_at(f, len, root + rel as u64 + 16, n * 8, MAX_READ)?;
        Some(
            (0..n)
                .filter_map(|i| {
                    let name = u32_at(&raw, i * 8)?;
                    let off = u32_at(&raw, i * 8 + 4)?;
                    Some(DirEntry {
                        id: (name & 0x8000_0000 == 0).then_some(name),
                        is_dir: off & 0x8000_0000 != 0,
                        offset: off & 0x7fff_ffff,
                    })
                })
                .collect(),
        )
    };
    // name level -> language level -> data entry (first language)
    let data_of = |f: &mut File, entry: &DirEntry, max: usize| -> Option<Vec<u8>> {
        let mut e = DirEntry { id: entry.id, is_dir: entry.is_dir, offset: entry.offset };
        for _ in 0..3 {
            if !e.is_dir {
                let d = read_at(f, len, root + e.offset as u64, 16, MAX_READ)?;
                let (rva, size) = (u32_at(&d, 0)?, u32_at(&d, 4)? as usize);
                return read_at(f, len, to_offset(rva)?, size, max);
            }
            e = read_dir(f, e.offset)?.into_iter().next()?;
        }
        None
    };
    let types = read_dir(&mut f, 0)?;
    let groups_dir = types.iter().find(|e| e.id == Some(14) && e.is_dir)?; // RT_GROUP_ICON
    let icons_dir = types.iter().find(|e| e.id == Some(3) && e.is_dir)?; // RT_ICON
    let groups = read_dir(&mut f, groups_dir.offset)?;
    let icons = read_dir(&mut f, icons_dir.offset)?;
    for group in groups.iter().take(3) {
        let Some(g) = data_of(&mut f, group, MAX_READ) else { continue };
        if u16_at(&g, 2) != Some(1) {
            continue;
        }
        let count = (u16_at(&g, 4)? as usize).min(64);
        // (pixels, bit depth, resource id)
        let mut entries: Vec<(u32, u16, u32)> = (0..count)
            .filter_map(|i| {
                let b = g.get(6 + i * 14..6 + i * 14 + 14)?;
                let w = if b[0] == 0 { 256 } else { b[0] as u32 };
                let h = if b[1] == 0 { 256 } else { b[1] as u32 };
                Some((w * h, u16_at(b, 6)?, u16_at(b, 12)? as u32))
            })
            .collect();
        entries.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        let images: Vec<Vec<u8>> = entries
            .iter()
            .filter_map(|&(_, _, id)| icons.iter().find(|e| e.id == Some(id)))
            .take(4)
            .filter_map(|e| data_of(&mut f, e, MAX_ICON_BYTES))
            .collect();
        if !images.is_empty() {
            return Some(images);
        }
    }
    None
}

// ---------------------------------------------------------------- decoding

/// Decodes one image of an icon resource: PNG as it is, a bitmap wrapped in
/// a one-image `.ico` so the ICO decoder reads its colour and mask data.
fn decode_icon_image(data: &[u8]) -> Option<DynamicImage> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return image::load_from_memory_with_format(data, ImageFormat::Png).ok();
    }
    let w = u32_at(data, 4)? as i32;
    let h = (u32_at(data, 8)? as i32) / 2; // colour + mask
    if !(1..=256).contains(&w) || !(1..=256).contains(&h) {
        return None;
    }
    let bits = u16_at(data, 14)?;
    let mut ico = Vec::with_capacity(22 + data.len());
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    ico.push(if w == 256 { 0 } else { w as u8 });
    ico.push(if h == 256 { 0 } else { h as u8 });
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&bits.to_le_bytes());
    ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(data);
    image::load_from_memory_with_format(&ico, ImageFormat::Ico).ok()
}

/// Brings an icon to the stored size: large icons are scaled down smoothly,
/// small ones (16 or 32 px) enlarged by whole pixels so they stay crisp.
pub fn render_icon(img: &DynamicImage) -> RgbaImage {
    let (w, h) = (img.width().max(1), img.height().max(1));
    let scaled = if w >= ICON_SIZE || h >= ICON_SIZE {
        img.resize(ICON_SIZE, ICON_SIZE, FilterType::Lanczos3)
    } else {
        let k = (ICON_SIZE / w).min(ICON_SIZE / h).max(1);
        img.resize_exact(w * k, h * k, FilterType::Nearest)
    };
    let mut canvas = RgbaImage::new(ICON_SIZE, ICON_SIZE);
    let x = (ICON_SIZE - scaled.width().min(ICON_SIZE)) / 2;
    let y = (ICON_SIZE - scaled.height().min(ICON_SIZE)) / 2;
    image::imageops::overlay(&mut canvas, &scaled.to_rgba8(), x as i64, y as i64);
    canvas
}

/// The icon of an installation: the GOG `.ico` when there is one, otherwise
/// the icon of the given program.
pub fn read_icon(install_dir: &Path, exe: Option<&Path>) -> Option<RgbaImage> {
    if let Some(ico) = find_gog_ico(install_dir) {
        let ok_size = fs::metadata(&ico).map(|m| m.len() as usize <= MAX_ICON_BYTES).unwrap_or(false);
        if ok_size {
            if let Ok(bytes) = fs::read(&ico) {
                if let Ok(img) = image::load_from_memory_with_format(&bytes, ImageFormat::Ico) {
                    return Some(render_icon(&img));
                }
            }
        }
    }
    let images = exe_icon_images(exe?)?;
    images.iter().find_map(|d| decode_icon_image(d)).map(|img| render_icon(&img))
}

// ---------------------------------------------------------------- cache

fn icon_file_ok(layout: &ArchiveLayout, rel: &str) -> bool {
    paths::is_safe_relative(rel) && rel.starts_with("Data/Thumbnails/icon-") && layout.root.join(rel).is_file()
}

/// Reads the icon of a game installed on this computer and caches it.
/// Without `force`, a game whose cached icon exists is skipped. Returns
/// whether a new icon was stored.
pub fn update_icon(conn: &Connection, layout: &ArchiveLayout, machine: &str, game_id: i64, force: bool) -> Result<bool> {
    let current: Option<String> = conn.query_row("SELECT icon_path FROM games WHERE id = ?1", [game_id], |r| r.get(0))?;
    if !force && current.as_deref().is_some_and(|p| icon_file_ok(layout, p)) {
        return Ok(false);
    }
    let Ok(dir) = installs::install_dir(conn, machine, game_id) else { return Ok(false) };
    let exe = installs::launch_target(conn, machine, game_id).ok().map(|t| t.exe);
    let Some(icon) = read_icon(&dir, exe.as_deref()) else { return Ok(false) };
    let enc = webp::Encoder::from_rgba(icon.as_raw(), icon.width(), icon.height());
    let bytes = enc.encode_lossless().to_vec();
    if bytes.is_empty() {
        return Err(CoreError::Image("WebP encoding of the icon failed.".into()));
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let name = format!("icon-{}.webp", &id[..16]);
    fs::create_dir_all(layout.thumbnails_dir())?;
    crate::images::write_atomic(&layout.thumbnails_dir().join(&name), &bytes)?;
    let rel = format!("Data/Thumbnails/{name}");
    conn.execute("UPDATE games SET icon_path = ?1 WHERE id = ?2", params![rel, game_id])?;
    if let Some(old) = current {
        if icon_file_ok(layout, &old) {
            let _ = fs::remove_file(layout.root.join(&old));
        }
    }
    Ok(true)
}

/// Caches the icons of the games installed on this computer that have none
/// yet. A game whose icon cannot be read is simply left without one.
pub fn refresh_icons(conn: &Connection, layout: &ArchiveLayout, machine: &str) -> Result<usize> {
    let ids: Vec<i64> = conn
        .prepare("SELECT game_id FROM installations WHERE machine = ?1")?
        .query_map([installs::machine_key(machine)], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut stored = 0;
    for id in ids {
        if update_icon(conn, layout, machine, id, false).unwrap_or(false) {
            stored += 1;
        }
    }
    Ok(stored)
}
