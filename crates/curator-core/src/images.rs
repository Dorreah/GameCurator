//! Artwork import: JPG/PNG/WebP in, optimized lossy WebP out.
//!
//! * `Data/Artwork/Covers/cover-<id>.webp` - the whole image (max 1200x1600)
//! * `Data/Thumbnails/cover-<id>.webp`     - what the card frame shows: 360x480
//!   (3:4, like the frame), rendered from the part of the cover the user chose
//!   (pan and zoom in the artwork editor; centred by default). Sharp up to
//!   ~150 % display scaling with the large card size.
//!
//! The crop can be changed later at any time: the cover keeps the whole
//! image and only the thumbnail is rendered again.
//!
//! Screenshots:
//! * `Data/Screenshots/game-<id>/shot-<id>.webp` - the picture (max 3840 px)
//! * `Data/Thumbnails/shot-<id>.webp`            - 16:9 preview (480x270)
//!
//! The user's original image is only read, never modified.

use std::fs;
use std::path::Path;

use image::{imageops::FilterType, DynamicImage, ImageReader, Limits, Rgba, RgbaImage};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::archive::ArchiveLayout;
use crate::{paths, CoreError, Result};

pub const ALLOWED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];
const MAX_SOURCE_BYTES: u64 = 50 * 1024 * 1024;
const COVER_MAX: (u32, u32) = (1200, 1600);
const SHOT_MAX: (u32, u32) = (3840, 3840);
/// Screenshot previews are 16:9 like most games.
pub const SHOT_THUMB: (u32, u32) = (480, 270);

/// The card frame is 3:4; the thumbnail is exactly what the frame shows.
pub const THUMB_SIZE: (u32, u32) = (360, 480);

/// Part of the cover shown in the card frame, as fractions of the cover's
/// width and height. It may reach outside the image (zoomed out so that the
/// whole picture fits); the uncovered area is filled with a dark tone of the
/// picture's average colour.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Crop {
    /// The largest centred 3:4 area (what the frame shows by default).
    pub fn centred(width: u32, height: u32) -> Crop {
        let (w, h) = (width.max(1) as f64, height.max(1) as f64);
        let frame = THUMB_SIZE.0 as f64 / THUMB_SIZE.1 as f64;
        if w / h > frame {
            let cw = h * frame / w;
            Crop { x: (1.0 - cw) / 2.0, y: 0.0, w: cw, h: 1.0 }
        } else {
            let ch = w / frame / h;
            Crop { x: 0.0, y: (1.0 - ch) / 2.0, w: 1.0, h: ch }
        }
    }

    pub fn validate(&self) -> Result<()> {
        let finite = [self.x, self.y, self.w, self.h].iter().all(|v| v.is_finite());
        let sized = (0.01..=25.0).contains(&self.w) && (0.01..=25.0).contains(&self.h);
        let overlaps = self.x < 1.0 && self.y < 1.0 && self.x + self.w > 0.0 && self.y + self.h > 0.0;
        if finite && sized && overlaps {
            Ok(())
        } else {
            Err(CoreError::invalid("The chosen part of the artwork is not valid. Please adjust it again."))
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(s: Option<&str>) -> Option<Crop> {
        s.and_then(|s| serde_json::from_str::<Crop>(s).ok()).filter(|c| c.validate().is_ok())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessedArtwork {
    pub artwork_path: String,
    pub thumbnail_path: String,
    pub width: u32,
    pub height: u32,
    pub thumbnail_bytes: u64,
}

fn encode_webp(img: &DynamicImage, quality: f32) -> Result<Vec<u8>> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let enc = webp::Encoder::from_rgba(rgba.as_raw(), w, h);
    let mem = enc.encode_simple(false, quality).map_err(|e| CoreError::Image(format!("WebP encoding failed: {e:?}")))?;
    Ok(mem.to_vec())
}

fn encode_webp_rgb(img: &DynamicImage, quality: f32) -> Result<Vec<u8>> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let enc = webp::Encoder::from_rgb(rgb.as_raw(), w, h);
    let mem = enc.encode_simple(false, quality).map_err(|e| CoreError::Image(format!("WebP encoding failed: {e:?}")))?;
    Ok(mem.to_vec())
}

fn fit(img: &DynamicImage, (mw, mh): (u32, u32)) -> DynamicImage {
    if img.width() <= mw && img.height() <= mh {
        img.clone()
    } else {
        img.resize(mw, mh, FilterType::CatmullRom)
    }
}

fn background(img: &DynamicImage) -> Rgba<u8> {
    let small = img.thumbnail(8, 8).to_rgba8();
    let n = (small.width() * small.height()).max(1) as u64;
    let mut sum = [0u64; 3];
    for p in small.pixels() {
        for (i, s) in sum.iter_mut().enumerate() {
            *s += p[i] as u64;
        }
    }
    // a dark tone of the average colour, so letterboxing looks intentional
    let c = |v: u64| ((v / n) as f64 * 0.35) as u8;
    Rgba([c(sum[0]), c(sum[1]), c(sum[2]), 255])
}

/// Renders what the card frame shows for the given crop (360x480).
pub fn render_thumbnail(img: &DynamicImage, crop: &Crop) -> RgbaImage {
    let (ow, oh) = THUMB_SIZE;
    let (w, h) = (img.width() as f64, img.height() as f64);
    let (px, py, pw, ph) = (crop.x * w, crop.y * h, crop.w * w, crop.h * h);
    let (kx, ky) = (ow as f64 / pw, oh as f64 / ph);
    let mut canvas = RgbaImage::from_pixel(ow, oh, background(img));
    // visible part of the source image
    let sx = px.max(0.0).floor() as u32;
    let sy = py.max(0.0).floor() as u32;
    let ex = ((px + pw).min(w).ceil() as u32).min(img.width());
    let ey = ((py + ph).min(h).ceil() as u32).min(img.height());
    if ex > sx && ey > sy {
        let (sw, sh) = (ex - sx, ey - sy);
        let dw = ((sw as f64) * kx).round().max(1.0) as u32;
        let dh = ((sh as f64) * ky).round().max(1.0) as u32;
        let dx = ((sx as f64 - px) * kx).round() as i64;
        let dy = ((sy as f64 - py) * ky).round() as i64;
        let part = img.crop_imm(sx, sy, sw, sh).resize_exact(dw, dh, FilterType::CatmullRom).to_rgba8();
        image::imageops::overlay(&mut canvas, &part, dx, dy);
    }
    canvas
}

fn new_name() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("cover-{}.webp", &id[..16])
}

/// Renders the thumbnail of a stored cover again with a new crop. Writes a
/// new file (so no old picture stays in any cache) and returns its stored
/// path; the caller removes the previous thumbnail.
pub fn rerender_thumbnail(layout: &ArchiveLayout, artwork_rel: &str, crop: &Crop) -> Result<String> {
    crop.validate()?;
    if !paths::is_safe_relative(artwork_rel) || !artwork_rel.starts_with("Data/Artwork/Covers/") {
        return Err(CoreError::PathOutsideAllowed);
    }
    let path = layout.root.join(artwork_rel);
    let img = ImageReader::open(&path)
        .map_err(|_| CoreError::invalid("The artwork file is missing. Please choose the image again."))?
        .with_guessed_format()?
        .decode()
        .map_err(|e| CoreError::Image(e.to_string()))?;
    let thumb = DynamicImage::ImageRgba8(render_thumbnail(&img, crop));
    let bytes = encode_webp(&thumb, 80.0)?;
    fs::create_dir_all(layout.thumbnails_dir())?;
    let name = new_name();
    write_atomic(&layout.thumbnails_dir().join(&name), &bytes)?;
    Ok(format!("Data/Thumbnails/{name}"))
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn decode(source: &Path) -> Result<DynamicImage> {
    let ext = source.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(CoreError::invalid("Unsupported image format. Please use JPG, PNG or WebP."));
    }
    let meta = fs::metadata(source).map_err(|_| CoreError::invalid("The image file cannot be read."))?;
    if meta.len() > MAX_SOURCE_BYTES {
        return Err(CoreError::invalid("The image is too large (maximum 50 MB)."));
    }
    let mut reader = ImageReader::open(source)?.with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_000);
    limits.max_image_height = Some(16_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|e| CoreError::Image(e.to_string()))
}

/// Converts the chosen image into cover + thumbnail WebP files inside the
/// archive and returns their stored (archive-relative) paths.
pub fn process_artwork(layout: &ArchiveLayout, source: &Path) -> Result<ProcessedArtwork> {
    store_artwork(layout, &decode(source)?)
}

/// The same for an image received as bytes (a cover from GOG.com): only
/// JPG, PNG and WebP, with the same size limits as a chosen file.
pub fn process_artwork_bytes(layout: &ArchiveLayout, bytes: &[u8]) -> Result<ProcessedArtwork> {
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err(CoreError::invalid("The image is too large (maximum 50 MB)."));
    }
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    if !matches!(reader.format(), Some(image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP)) {
        return Err(CoreError::invalid("Unsupported image format. Please use JPG, PNG or WebP."));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_000);
    limits.max_image_height = Some(16_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| CoreError::Image(e.to_string()))?;
    store_artwork(layout, &img)
}

fn store_artwork(layout: &ArchiveLayout, img: &DynamicImage) -> Result<ProcessedArtwork> {
    let name = new_name();

    let cover = fit(img, COVER_MAX);
    let thumb = DynamicImage::ImageRgba8(render_thumbnail(&cover, &Crop::centred(cover.width(), cover.height())));
    let cover_bytes = encode_webp(&cover, 88.0)?;
    let thumb_bytes = encode_webp(&thumb, 80.0)?;

    fs::create_dir_all(layout.covers_dir())?;
    fs::create_dir_all(layout.thumbnails_dir())?;
    write_atomic(&layout.covers_dir().join(&name), &cover_bytes)?;
    write_atomic(&layout.thumbnails_dir().join(&name), &thumb_bytes)?;

    Ok(ProcessedArtwork {
        artwork_path: format!("Data/Artwork/Covers/{name}"),
        thumbnail_path: format!("Data/Thumbnails/{name}"),
        width: cover.width(),
        height: cover.height(),
        thumbnail_bytes: thumb_bytes.len() as u64,
    })
}

/// Deletes artwork files created by Game Curator. Only paths inside
/// `Data/Artwork/` and `Data/Thumbnails/` are ever touched.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessedScreenshot {
    pub image_path: String,
    pub thumbnail_path: String,
    pub width: u32,
    pub height: u32,
}

/// Stores a screenshot of a game: an optimized WebP copy (quality 90, up to
/// 3840 px, so 4K stays 4K) and a 16:9 preview.
pub fn process_screenshot(layout: &ArchiveLayout, game_id: i64, source: &Path) -> Result<ProcessedScreenshot> {
    let img = decode(source)?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let name = format!("shot-{}.webp", &id[..16]);
    let pic = fit(&img, SHOT_MAX);
    let (tw, th) = SHOT_THUMB;
    let thumb = pic.resize_to_fill(tw, th, FilterType::CatmullRom);
    let pic_bytes = encode_webp_rgb(&pic, 90.0)?;
    let thumb_bytes = encode_webp_rgb(&thumb, 80.0)?;
    let dir = layout.screenshots_dir().join(format!("game-{game_id}"));
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(layout.thumbnails_dir())?;
    write_atomic(&dir.join(&name), &pic_bytes)?;
    if let Err(e) = write_atomic(&layout.thumbnails_dir().join(&name), &thumb_bytes) {
        let _ = fs::remove_file(dir.join(&name));
        return Err(e);
    }
    Ok(ProcessedScreenshot {
        image_path: format!("Data/Screenshots/game-{game_id}/{name}"),
        thumbnail_path: format!("Data/Thumbnails/{name}"),
        width: pic.width(),
        height: pic.height(),
    })
}

/// Removes the stored copy and preview of a screenshot (never anything else).
pub fn remove_screenshot_files(layout: &ArchiveLayout, image: &str, thumb: &str) {
    for (p, prefix) in [(image, "Data/Screenshots/"), (thumb, "Data/Thumbnails/")] {
        if paths::is_safe_relative(p) && p.starts_with(prefix) {
            let _ = fs::remove_file(layout.root.join(p));
        }
    }
}

pub fn remove_artwork_files(layout: &ArchiveLayout, artwork: Option<&str>, thumb: Option<&str>) {
    for (p, prefix) in [(artwork, "Data/Artwork/"), (thumb, "Data/Thumbnails/")] {
        if let Some(p) = p {
            if paths::is_safe_relative(p) && p.starts_with(prefix) {
                let _ = fs::remove_file(layout.root.join(p));
            }
        }
    }
}

/// Removes `cover-*.webp` files that no game references (e.g. artwork chosen
/// in a form that was then cancelled), and previews and cached icons nobody
/// uses any more. Returns the number of removed files.
pub fn cleanup_orphans(conn: &Connection, layout: &ArchiveLayout) -> Result<usize> {
    let mut used = std::collections::HashSet::new();
    let mut stmt = conn.prepare(
        "SELECT artwork_path, thumbnail_path FROM games UNION ALL SELECT icon_path, NULL FROM games
         UNION ALL SELECT image_path, thumbnail_path FROM screenshots",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)))?;
    for row in rows {
        let (a, t) = row?;
        for p in [a, t].into_iter().flatten() {
            if let Some(name) = p.rsplit('/').next() {
                used.insert(name.to_string());
            }
        }
    }
    let mut removed = 0;
    for dir in [layout.covers_dir(), layout.thumbnails_dir()] {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            // covers and previews can always be made again; screenshot copies
            // themselves are never removed automatically (only unfinished .tmp)
            let ours = (name.starts_with("cover-") || name.starts_with("shot-") || name.starts_with("icon-"))
                && (name.ends_with(".webp") || name.ends_with(".tmp"));
            if ours && !used.contains(&name) && fs::remove_file(e.path()).is_ok() {
                removed += 1;
            }
        }
    }
    if let Ok(games) = fs::read_dir(layout.screenshots_dir()) {
        for dir in games.flatten() {
            for e in fs::read_dir(dir.path()).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("shot-") && name.ends_with(".tmp") && fs::remove_file(e.path()).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    Ok(removed)
}

/// Serves an image of the archive to the UI. Only files under
/// `Data/Artwork/`, `Data/Thumbnails/` and `Data/Screenshots/` are readable.
pub fn read_archive_image(layout: &ArchiveLayout, rel: &str) -> Result<(Vec<u8>, &'static str)> {
    let allowed = ["Data/Artwork/", "Data/Thumbnails/", "Data/Screenshots/"].iter().any(|p| rel.starts_with(p));
    if !paths::is_safe_relative(rel) || !allowed {
        return Err(CoreError::PathOutsideAllowed);
    }
    let path = layout.root.join(rel);
    let bytes = fs::read(&path).map_err(|_| CoreError::not_found("Image not found."))?;
    let mime = match path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "image/webp",
    };
    Ok((bytes, mime))
}
