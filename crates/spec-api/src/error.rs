use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpecError {
    #[error("spec not found: {0}")]
    NotFound(String),

    #[error("invalid slug: {0}")]
    InvalidSlug(String),

    #[error("invalid generated artifact: {0}")]
    InvalidGeneratedArtifact(String),

    #[error("duplicate slug: {0}")]
    DuplicateSlug(String),

    #[error("empty body update rejected for {0} (pass force=true to allow)")]
    EmptyBody(String),

    #[error("no-op body update rejected for {0}: content is unchanged")]
    NoOpUpdate(String),

    #[error("storage error: {0}")]
    Storage(#[from] memory_api::error::StorageError),

    #[error("schema validation: {0}")]
    Validation(#[from] memory_api::error::SchemaValidationError),

    #[error("serialization error: {0}")]
    Serialization(String),
}
