use std::path::PathBuf;

/// Errors produced by the validation-log store.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("log store root cannot be empty")]
    EmptyRoot,

    #[error("interoperability contract violation for {record_kind}: {detail}")]
    InteroperabilityContract {
        record_kind: String,
        detail: String,
    },

    #[error("identifier contains invalid path characters: {0}")]
    InvalidId(String),

    #[error("workspace slug contains invalid path characters: {0}")]
    InvalidWorkspaceSlug(String),

    #[error("log capture not found: {0}")]
    CaptureNotFound(String),

    #[error("runtime log session not found: {0}")]
    RuntimeSessionNotFound(String),

    #[error("failed to serialize log data for {path}: {source}")]
    Serialize {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to deserialize log data from {path}: {source}")]
    Deserialize {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}
