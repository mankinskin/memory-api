use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SchemaValidationError {
    #[error("required field missing: {0}")]
    MissingRequiredField(String),
    #[error("unknown state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },
    #[error("edge kind not allowed: {0}")]
    InvalidEdgeKind(String),
    #[error("required states not visited before '{target}': {missing:?}")]
    RequiredStatesNotVisited {
        target: String,
        missing: Vec<String>,
    },
}

#[derive(Debug, Error)]
pub enum QueryParseError {
    #[error("invalid query expression: {0}")]
    InvalidExpression(String),
}

#[derive(Debug, Error)]
pub enum StorageSchemaError {
    #[error(
        "schema version mismatch: found '{found}', expected '{expected}'. Action: run 'ticket scan --reindex' after migration or apply schema upgrade before writing"
    )]
    VersionMismatch { found: String, expected: String },
}

/// Runtime storage errors covering redb, filesystem, and search index operations.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("schema version mismatch: {0}")]
    SchemaMismatch(#[from] StorageSchemaError),
    #[error("schema validation: {0}")]
    Validation(#[from] SchemaValidationError),
    #[error("query parse: {0}")]
    QueryParse(#[from] QueryParseError),
    #[error("entity not found: {0}")]
    NotFound(Uuid),
    #[error("entity lease conflict: entity {ticket} held by {holder}")]
    LeaseConflict { ticket: Uuid, holder: String },
    #[error("dependency cycle detected between entities")]
    DependencyCycle,
    #[error("search index error: {0}")]
    SearchIndex(String),
    #[error("parse diagnostic: {path}: {reason}", path = path.display())]
    ParseError {
        path: std::path::PathBuf,
        reason: String,
    },
    #[error("schema file parse error: {path}: {reason}", path = path.display())]
    SchemaFileParse {
        path: std::path::PathBuf,
        reason: String,
    },
    #[error("protocol: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("{0}")]
    Other(String),
}

/// Structured errors for the canonical `TaskCommand` agent protocol.
///
/// Error codes map directly to the `code` field in the structured error envelope,
/// e.g. `validate.invalid_state`, `release.validation_not_passed`.
#[derive(Debug, Error)]
pub enum ProtocolError {
    // ── validation errors ─────────────────────────────────────────────────────
    #[error("validate.invalid_state: ticket {ticket} is in state '{actual}', expected '{expected}'")]
    ValidateInvalidState {
        ticket: Uuid,
        actual: String,
        expected: String,
    },
    #[error("validate.same_identity: validator and worker must have different identities (got '{identity}')")]
    ValidateSameIdentity { identity: String },
    #[error("validate.assignment_mismatch: validator_id does not match the assigned validator for this ticket")]
    ValidateAssignmentMismatch,
    #[error("validate.missing_evidence: evidence_refs must contain at least one entry")]
    ValidateMissingEvidence,
    // ── release errors ────────────────────────────────────────────────────────
    #[error("release.invalid_state: ticket {ticket} is in state '{actual}', expected '{expected}'")]
    ReleaseInvalidState {
        ticket: Uuid,
        actual: String,
        expected: String,
    },
    #[error("release.validation_not_passed: ticket {ticket} has validation_status '{status}'")]
    ReleaseValidationNotPassed { ticket: Uuid, status: String },
    #[error("release.assignment_chain_missing: assignment_chain must not be empty")]
    ReleaseAssignmentChainMissing,
    #[error("release.gates_not_satisfied: {0}")]
    ReleaseGatesNotSatisfied(String),
    #[error("release.merge_metadata_missing: merge_commit is required for promote")]
    ReleaseMergeMetadataMissing,
    #[error("release.target_not_found: no tickets found for target '{0}'")]
    ReleaseTargetNotFound(String),
    #[error("release.ticket_state_invalid: {0}")]
    ReleaseTicketStateInvalid(String),
}

/// Machine-readable error code extracted from a `ProtocolError`.
impl ProtocolError {
    pub fn code(&self) -> &'static str {
        match self {
            ProtocolError::ValidateInvalidState { .. } => "validate.invalid_state",
            ProtocolError::ValidateSameIdentity { .. } => "validate.same_identity",
            ProtocolError::ValidateAssignmentMismatch => "validate.assignment_mismatch",
            ProtocolError::ValidateMissingEvidence => "validate.missing_evidence",
            ProtocolError::ReleaseInvalidState { .. } => "release.invalid_state",
            ProtocolError::ReleaseValidationNotPassed { .. } => "release.validation_not_passed",
            ProtocolError::ReleaseAssignmentChainMissing => "release.assignment_chain_missing",
            ProtocolError::ReleaseGatesNotSatisfied(_) => "release.gates_not_satisfied",
            ProtocolError::ReleaseMergeMetadataMissing => "release.merge_metadata_missing",
            ProtocolError::ReleaseTargetNotFound(_) => "release.target_not_found",
            ProtocolError::ReleaseTicketStateInvalid(_) => "release.ticket_state_invalid",
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        StorageError::Database(e.to_string())
    }
}
