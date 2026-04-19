use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpecError {
    #[error("spec not found: {0}")]
    NotFound(String),

    #[error("invalid slug: {0}")]
    InvalidSlug(String),

    #[error("duplicate slug: {0}")]
    DuplicateSlug(String),

    #[error("storage error: {0}")]
    Storage(#[from] memory_api::error::StorageError),

    #[error("serialization error: {0}")]
    Serialization(String),
}
