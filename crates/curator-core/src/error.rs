//! Error type shared by all core modules.
//!
//! Messages are written for end users: they are shown in the UI as-is.

use rusqlite::ErrorCode;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image could not be processed: {0}")]
    Image(String),

    #[error("Settings file error: {0}")]
    Json(#[from] serde_json::Error),

    /// Validation problem caused by user input. The message is shown directly.
    #[error("{0}")]
    Invalid(String),

    #[error("{0}")]
    NotFound(String),

    #[error("The archive database is not available. Is the external drive still connected?")]
    ArchiveUnavailable,

    #[error("Access denied: the path is outside the allowed folder.")]
    PathOutsideAllowed,

    #[error("The operation was cancelled.")]
    Cancelled,
}

impl CoreError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        CoreError::Invalid(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        CoreError::NotFound(msg.into())
    }

    /// True when the error indicates that the database file can no longer be
    /// reached (drive unplugged, file deleted, read-only media...). The caller
    /// should drop the connection so that it is reopened on the next request.
    pub fn is_storage_failure(&self) -> bool {
        match self {
            CoreError::ArchiveUnavailable => true,
            CoreError::Db(rusqlite::Error::SqliteFailure(e, _)) => matches!(
                e.code,
                ErrorCode::SystemIoFailure
                    | ErrorCode::CannotOpen
                    | ErrorCode::ReadOnly
                    | ErrorCode::NotADatabase
                    | ErrorCode::DatabaseCorrupt
                    | ErrorCode::DiskFull
            ),
            _ => false,
        }
    }

    /// True for UNIQUE / FOREIGN KEY / CHECK violations.
    pub fn is_constraint(&self) -> bool {
        matches!(self, CoreError::Db(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::ConstraintViolation)
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
