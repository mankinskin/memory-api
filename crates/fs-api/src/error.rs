use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FsApiError {
    #[error("{0}")]
    InvalidRequest(String),

    #[error("path traversal detected: '{path}'")]
    PathTraversal { path: PathBuf },

    #[error("path not found: '{path}'")]
    PathNotFound { path: PathBuf },

    #[error("cannot read directory '{path}': {source}")]
    CannotReadDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot read metadata for '{path}': {source}")]
    CannotReadMetadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot move file from '{from}' to '{to}': {source}")]
    CannotMoveFile {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot copy file from '{from}' to '{to}': {source}")]
    CannotCopyFile {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot delete file '{path}': {source}")]
    CannotDeleteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot delete directory '{path}': {source}")]
    CannotDeleteDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
