use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("json error in {path}: {source}")]
    Json { path: PathBuf, source: serde_json::Error },
    #[error("not a board: {0}")]
    NotABoard(PathBuf),
    #[error("board is read-only: {0}")]
    ReadOnly(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid operation: {0}")]
    Invalid(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io { path: path.into(), source }
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        Error::Invalid(msg.into())
    }
    pub fn not_found(what: impl Into<String>) -> Self {
        Error::NotFound(what.into())
    }
    /// Stable machine-readable code for the UI.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io { .. } => "io",
            Error::Json { .. } => "json",
            Error::NotABoard(_) => "not_a_board",
            Error::ReadOnly(_) => "read_only",
            Error::NotFound(_) => "not_found",
            Error::Invalid(_) => "invalid",
            Error::Conflict(_) => "conflict",
            Error::Other(_) => "other",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
