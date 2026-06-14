use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocError {
    #[error("failed to parse cargo metadata: {0}")]
    CargoMetadata(String),

    #[error("failed to read cargo metadata from {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cargo metadata package manifest has no parent directory: {0}")]
    InvalidManifestPath(PathBuf),

    #[error("cargo metadata did not describe any workspace packages")]
    EmptyWorkspace,
}
