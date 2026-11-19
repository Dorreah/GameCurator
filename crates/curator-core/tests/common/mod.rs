#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use curator_core::archive::{self, OpenedArchive};
use curator_core::games::{self, GameInput};

pub struct Fixture {
    pub tmp: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        Fixture { tmp: tempfile::tempdir().expect("tempdir") }
    }
    /// Simulated drive: <tmp>/<drive>
    pub fn drive(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }
    pub fn archive_root(&self, drive: &str) -> PathBuf {
        self.drive(drive).join("GAME_CURATOR")
    }
    pub fn open(&self, drive: &str) -> OpenedArchive {
        archive::open_archive(&self.archive_root(drive)).expect("open archive")
    }
}

pub fn touch(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![b'x'; bytes]).unwrap();
}

/// A GOG-like game folder with installer, data part, manual and soundtrack.
pub fn make_game_folder(dir: &Path, slug: &str) {
    touch(&dir.join(format!("setup_{slug}_1.0.2.exe")), 2048);
    touch(&dir.join(format!("setup_{slug}_1.0.2-1.bin")), 8192);
    touch(&dir.join("Manual.pdf"), 512);
    touch(&dir.join("Soundtrack").join("01 - Theme.mp3"), 300);
    touch(&dir.join("Soundtrack").join("02 - Battle.flac"), 300);
    touch(&dir.join("extras").join("artbook.zip"), 100);
}

pub fn input(title: &str, folder: &Path) -> GameInput {
    GameInput { title: title.into(), folder_path: folder.to_string_lossy().into_owned(), ..Default::default() }
}

pub fn add_game(a: &mut OpenedArchive, title: &str, folder: &Path) -> i64 {
    let layout = a.layout.clone();
    games::create_game(a.db.conn().unwrap(), &layout, &input(title, folder)).expect("create game")
}

pub fn write_png(path: &Path, w: u32, h: u32) {
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    img.save(path).unwrap();
}
