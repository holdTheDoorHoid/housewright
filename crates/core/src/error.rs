//! Error type for the core.

use thiserror::Error;

/// Anything that can go wrong in the core.
#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("date/time error: {0}")]
    Time(#[from] jiff::Error),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("template {file}: {message}")]
    Template { file: String, message: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("import failed: {0}")]
    Import(String),
}

/// Result alias used throughout the core.
pub type Result<T, E = Error> = std::result::Result<T, E>;
