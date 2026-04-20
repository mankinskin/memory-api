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

    #[error("schema validation: {0}")]
    Validation(#[from] memory_api::error::SchemaValidationError),

    #[error("serialization error: {0}")]
    Serialization(String),
}
