use std::path::PathBuf;

/// Errors produced by the test-result store.
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("test store root cannot be empty")]
    EmptyRoot,

    #[error("identifier contains invalid path characters: {0}")]
    InvalidId(String),

    #[error("workspace slug contains invalid path characters: {0}")]
    InvalidWorkspaceSlug(String),

    #[error("validation spec not found: {0}")]
    SpecNotFound(String),

    #[error("validation execution not found: {0}")]
    ExecutionNotFound(String),

    #[error("failed to serialize test data for {path}: {source}")]
    Serialize {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to deserialize test data from {path}: {source}")]
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
