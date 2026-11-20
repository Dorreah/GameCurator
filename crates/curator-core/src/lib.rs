//! # curator-core
//!
//! Everything Game Curator does that does not need a window: the SQLite
//! catalog, portable paths, archive identification, the archive scanner,
//! the file catalog, artwork processing, journal storage, installed games
//! and backups.
//!
//! The Tauri application (`src-tauri`) is a thin layer that exposes these
//! functions as commands and performs OS interactions (dialogs, opening
//! files). Keeping the core GUI-free makes it fully testable with
//! `cargo test`.

pub mod additions;
pub mod archive;
pub mod backup;
pub mod config;
pub mod db;
pub mod error;
pub mod files;
pub mod games;
pub mod icons;
pub mod images;
pub mod installs;
pub mod journal;
pub mod media;
pub mod paths;
pub mod scanner;
pub mod screenshots;
pub mod text;

pub use error::{CoreError, Result};
/// Re-exported so the application layer uses exactly the same SQLite build.
pub use rusqlite;

pub const APP_NAME: &str = "Game Curator";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
