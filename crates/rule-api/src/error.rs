use memory_kernel::error::StorageError;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RuleError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("rule not found: {0}")]
    NotFound(String),
    #[error("duplicate rule slug: {0}")]
    DuplicateSlug(String),
    #[error("invalid rule slug: {0}")]
    InvalidSlug(String),
    #[error("rule UUID prefix is ambiguous: {0}")]
    AmbiguousPrefix(String),
    #[error("rule asset operation failed: {0}")]
    Asset(String),
    #[error(
        "rule {slug} matched multiple nodes while rendering target {target} (node: {node})"
    )]
    DuplicateRenderRule {
        target: String,
        node: String,
        slug: String,
    },
    #[error("rule id mismatch: expected {expected}, got {actual}")]
    IdMismatch { expected: Uuid, actual: Uuid },
}
