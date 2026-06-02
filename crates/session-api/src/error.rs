use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session capture is missing a session id")]
    MissingSessionId,

    #[error("session capture did not include any turns")]
    EmptyTurns,

    #[error("session store root cannot be empty")]
    EmptyStoreRoot,

    #[error("session id contains invalid path characters: {0}")]
    InvalidSessionId(String),

    #[error("workspace slug contains invalid path characters: {0}")]
    InvalidWorkspaceSlug(String),

    #[error("store path has no parent directory: {0}")]
    InvalidStorePath(PathBuf),

    #[error("failed to serialize session data for {path}: {source}")]
    Serialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("filesystem operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}