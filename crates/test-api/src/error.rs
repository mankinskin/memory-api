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

    #[error("benchmark execution not found: {0}")]
    BenchmarkNotFound(String),

    #[error("failed to parse budget table {path}: {detail}")]
    BudgetParse { path: PathBuf, detail: String },

    #[error("failed to ingest Criterion estimates {path}: {detail}")]
    CriterionIngest { path: PathBuf, detail: String },

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
