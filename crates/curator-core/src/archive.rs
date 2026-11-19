//! Archive layout on the external drive and archive identification.
//!
//! ```text
//! GAME_CURATOR/            <- archive root (folder of GameCurator.exe)
//!   GameCurator.exe
//!   README.txt
//!   .gamecurator-id
//!   Data/  library.db, config.json, Artwork/{Covers,Backgrounds,Logos},
//!          Thumbnails/, Journals/, Backups/, WebView/ (browser cache)
//!   Games/
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::db::{self, Database};
use crate::{CoreError, Result};

pub const ID_FILE: &str = ".gamecurator-id";
const SETTING_ARCHIVE_ID: &str = "archive_id";

#[derive(Clone, Debug)]
pub struct ArchiveLayout {
    pub root: PathBuf,
}

impl ArchiveLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        ArchiveLayout { root: root.into() }
    }
    pub fn data_dir(&self) -> PathBuf {
        self.root.join("Data")
    }
    pub fn db_path(&self) -> PathBuf {
        self.data_dir().join("library.db")
    }
    pub fn config_path(&self) -> PathBuf {
        self.data_dir().join("config.json")
    }
    pub fn artwork_dir(&self) -> PathBuf {
        self.data_dir().join("Artwork")
    }
    pub fn covers_dir(&self) -> PathBuf {
        self.artwork_dir().join("Covers")
    }
    pub fn thumbnails_dir(&self) -> PathBuf {
        self.data_dir().join("Thumbnails")
    }
    pub fn journals_dir(&self) -> PathBuf {
        self.data_dir().join("Journals")
    }
    pub fn screenshots_dir(&self) -> PathBuf {
        self.data_dir().join("Screenshots")
    }
    pub fn additions_dir(&self) -> PathBuf {
        self.data_dir().join("Additions")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir().join("Backups")
    }
    /// WebView2 / WebKit profile (cache, local storage). Kept on the external
    /// drive so nothing persistent is written into %LOCALAPPDATA%.
    pub fn webview_dir(&self) -> PathBuf {
        self.data_dir().join("WebView")
    }
    pub fn games_dir(&self) -> PathBuf {
        self.root.join("Games")
    }
    pub fn id_file(&self) -> PathBuf {
        self.root.join(ID_FILE)
    }

    pub fn ensure_structure(&self) -> Result<()> {
        for d in [
            self.data_dir(),
            self.covers_dir(),
            self.artwork_dir().join("Backgrounds"),
            self.artwork_dir().join("Logos"),
            self.thumbnails_dir(),
            self.journals_dir(),
            self.backups_dir(),
            self.games_dir(),
        ] {
            fs::create_dir_all(&d)?;
        }
        Ok(())
    }

    /// Quick check used by the status bar: is the drive (still) there?
    pub fn is_available(&self) -> bool {
        self.data_dir().is_dir() && self.db_path().is_file()
    }

    /// Verifies that the archive drive accepts writes (not read-only media).
    pub fn check_writable(&self) -> Result<()> {
        let probe = self.data_dir().join(".write-test");
        fs::write(&probe, b"ok").map_err(|e| {
            CoreError::invalid(format!("The archive folder is not writable ({e}). Is the drive write-protected?"))
        })?;
        let _ = fs::remove_file(&probe);
        Ok(())
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ArchiveIdStatus {
    /// Database and identification file match.
    Ok,
    /// New archive: identifier created now.
    Created,
    /// `.gamecurator-id` was missing and has been recreated from the database.
    IdFileRestored,
    /// The database belongs to another archive (e.g. copied from elsewhere).
    #[serde(rename_all = "camelCase")]
    Mismatch { db_id: String, file_id: String },
}

pub struct OpenedArchive {
    pub layout: ArchiveLayout,
    pub db: Database,
    pub archive_id: String,
    pub id_status: ArchiveIdStatus,
    pub db_created: bool,
}

pub fn read_id_file(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .find_map(|l| uuid::Uuid::parse_str(l).ok())
        .map(|u| u.to_string())
}

pub fn write_id_file(path: &Path, id: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(
        &tmp,
        format!("# Game Curator archive identifier. Do not edit or delete this file.\n{id}\n"),
    )?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Opens the archive located at `root`: creates the folder structure, opens
/// or creates `Data/library.db` and verifies the archive identifier.
pub fn open_archive(root: &Path) -> Result<OpenedArchive> {
    let layout = ArchiveLayout::new(root);
    layout.ensure_structure()?;
    let db_created = !layout.db_path().exists();
    let mut db = Database::open(&layout.db_path())?;
    let file_id = read_id_file(&layout.id_file());
    let conn = db.conn()?;
    let db_id = db::get_setting(conn, SETTING_ARCHIVE_ID)?;

    let (archive_id, id_status) = match (db_id, file_id) {
        (None, file_id) => {
            let created = file_id.is_none();
            let id = file_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            db::set_setting(conn, SETTING_ARCHIVE_ID, &id)?;
            db::set_setting(conn, "created_at", &db::now_iso(conn)?)?;
            if created {
                write_id_file(&layout.id_file(), &id)?;
            }
            (id, if created { ArchiveIdStatus::Created } else { ArchiveIdStatus::Ok })
        }
        (Some(db_id), None) => {
            write_id_file(&layout.id_file(), &db_id)?;
            (db_id, ArchiveIdStatus::IdFileRestored)
        }
        (Some(db_id), Some(file_id)) if db_id == file_id => (db_id, ArchiveIdStatus::Ok),
        (Some(db_id), Some(file_id)) => {
            (file_id.clone(), ArchiveIdStatus::Mismatch { db_id, file_id })
        }
    };

    Ok(OpenedArchive { layout, db, archive_id, id_status, db_created })
}

/// Resolves a mismatch by assigning the database to the current archive.
pub fn adopt_database(layout: &ArchiveLayout, db: &mut Database) -> Result<String> {
    let id = match read_id_file(&layout.id_file()) {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            write_id_file(&layout.id_file(), &id)?;
            id
        }
    };
    db::set_setting(db.conn()?, SETTING_ARCHIVE_ID, &id)?;
    Ok(id)
}

pub fn archive_id_of(conn: &rusqlite::Connection) -> Result<Option<String>> {
    db::get_setting(conn, SETTING_ARCHIVE_ID)
}
