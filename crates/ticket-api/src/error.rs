use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SchemaValidationError {
	#[error("required field missing: {0}")]
	MissingRequiredField(String),
	#[error(
		"state '{state}' is not allowed by the schema; allowed states are [{allowed}]",
		allowed = .allowed.join(", "),
	)]
	OffSchemaState {
		state: String,
		allowed: Vec<String>,
	},
	#[error(
		"invalid state transition '{from}' -> '{to}'; current state '{from}' allows next states [{allowed}]; {path_hint}",
		allowed = .allowed_next.join(", "),
		path_hint = transition_path_hint(.intermediate.as_slice(), .to.as_str()),
	)]
	InvalidTransition {
		from: String,
		to: String,
		allowed_next: Vec<String>,
		intermediate: Vec<String>,
	},
	#[error("edge kind not allowed: {0}")]
	InvalidEdgeKind(String),
	#[error("required states not visited before '{target}': {missing:?}")]
	RequiredStatesNotVisited {
		target: String,
		missing: Vec<String>,
	},
	#[error(
		"invalid core part kind '{kind}': not a recognized core kind; valid core kinds are [{valid}]",
		valid = .valid_kinds.join(", "),
	)]
	InvalidCoreKind {
		kind: String,
		valid_kinds: Vec<String>,
	},
	#[error(
		"invalid ref kind '{kind}': not a recognized ref kind; valid ref kinds are [{valid}]",
		valid = .valid_kinds.join(", "),
	)]
	InvalidRefKind {
		kind: String,
		valid_kinds: Vec<String>,
	},
	#[error("invalid ref urn '{urn}' for kind '{kind}'")]
	InvalidRefUrn {
		kind: String,
		urn: String,
	},
	#[error(
		"unknown ticket type '{type_id}': no schema is registered for it; registered types are [{registered}]",
		registered = .registered.join(", "),
	)]
	UnknownType {
		type_id: String,
		registered: Vec<String>,
	},
	#[error(
		"part '{kind}' (id {part_id}) on ticket {ticket} is frozen by the '{freezing_state}' state and cannot be written directly; recover via (a) adding an 'amendment' part with supersedes = {part_id} to record the correction, or (b) transitioning the ticket back to a pre-'{freezing_state}' state to unfreeze it"
	)]
	FrozenPartWrite {
		ticket: Uuid,
		part_id: Uuid,
		kind: String,
		freezing_state: String,
	},
}

fn transition_path_hint(
	intermediate: &[String],
	to: &str,
) -> String {
	match intermediate.split_last() {
		Some((_target, waypoints)) if !waypoints.is_empty() => format!(
			"to reach '{to}', first transition through: {}",
			waypoints.join(" -> ")
		),
		_ => format!("no direct transition to '{to}' is available"),
	}
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
	#[error(
		"workspace not initialized at {path}: run the 'init' command to create a new workspace"
	)]
	WorkspaceNotFound { path: std::path::PathBuf },
	#[error("dependency cycle detected between entities")]
	DependencyCycle,
	#[error(
		"dependency not progressed: cannot move ticket {ticket} to '{target_state}' because dependency {dependency} is only at '{dependency_state}'"
	)]
	DependencyNotProgressed {
		ticket: Uuid,
		target_state: String,
		dependency: Uuid,
		dependency_state: String,
	},
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
	#[error(
		"part '{kind}' (id {part_id}) on ticket {ticket} is frozen by the '{freezing_state}' state and cannot be written directly; recover via (a) adding an 'amendment' part with supersedes = {part_id} to record the correction, or (b) transitioning the ticket back to a pre-'{freezing_state}' state to unfreeze it"
	)]
	FrozenPartWrite {
		ticket: Uuid,
		part_id: Uuid,
		kind: String,
		freezing_state: String,
	},
	#[error("{0}")]
	Other(String),
}

#[derive(Debug, Error)]
pub enum ProtocolError {
	#[error(
		"validate.invalid_state: ticket {ticket} is in state '{actual}', expected '{expected}'"
	)]
	ValidateInvalidState {
		ticket: Uuid,
		actual: String,
		expected: String,
	},
	#[error(
		"validate.same_identity: validator and worker must have different identities (got '{identity}')"
	)]
	ValidateSameIdentity { identity: String },
	#[error(
		"validate.assignment_mismatch: validator_id does not match the assigned validator for this ticket"
	)]
	ValidateAssignmentMismatch,
	#[error(
		"validate.missing_evidence: evidence_refs must contain at least one entry"
	)]
	ValidateMissingEvidence,
	#[error(
		"release.invalid_state: ticket {ticket} is in state '{actual}', expected '{expected}'"
	)]
	ReleaseInvalidState {
		ticket: Uuid,
		actual: String,
		expected: String,
	},
	#[error(
		"release.validation_not_passed: ticket {ticket} has validation_status '{status}'"
	)]
	ReleaseValidationNotPassed { ticket: Uuid, status: String },
	#[error(
		"release.assignment_chain_missing: assignment_chain must not be empty"
	)]
	ReleaseAssignmentChainMissing,
	#[error("release.gates_not_satisfied: {0}")]
	ReleaseGatesNotSatisfied(String),
	#[error(
		"release.merge_metadata_missing: merge_commit is required for promote"
	)]
	ReleaseMergeMetadataMissing,
	#[error("release.target_not_found: no tickets found for target '{0}'")]
	ReleaseTargetNotFound(String),
	#[error("release.ticket_state_invalid: {0}")]
	ReleaseTicketStateInvalid(String),
}

impl ProtocolError {
	pub fn code(&self) -> &'static str {
		match self {
			ProtocolError::ValidateInvalidState { .. } => {
				"validate.invalid_state"
			},
			ProtocolError::ValidateSameIdentity { .. } => {
				"validate.same_identity"
			},
			ProtocolError::ValidateAssignmentMismatch => {
				"validate.assignment_mismatch"
			},
			ProtocolError::ValidateMissingEvidence => {
				"validate.missing_evidence"
			},
			ProtocolError::ReleaseInvalidState { .. } => {
				"release.invalid_state"
			},
			ProtocolError::ReleaseValidationNotPassed { .. } => {
				"release.validation_not_passed"
			},
			ProtocolError::ReleaseAssignmentChainMissing => {
				"release.assignment_chain_missing"
			},
			ProtocolError::ReleaseGatesNotSatisfied(_) => {
				"release.gates_not_satisfied"
			},
			ProtocolError::ReleaseMergeMetadataMissing => {
				"release.merge_metadata_missing"
			},
			ProtocolError::ReleaseTargetNotFound(_) => {
				"release.target_not_found"
			},
			ProtocolError::ReleaseTicketStateInvalid(_) => {
				"release.ticket_state_invalid"
			},
		}
	}
}

impl From<rusqlite::Error> for StorageError {
	fn from(e: rusqlite::Error) -> Self {
		StorageError::Database(e.to_string())
	}
}

impl From<memory_kernel::error::SchemaValidationError>
	for SchemaValidationError
{
	fn from(value: memory_kernel::error::SchemaValidationError) -> Self {
		match value {
			memory_kernel::error::SchemaValidationError::MissingRequiredField(
				field,
			) => Self::MissingRequiredField(field),
			memory_kernel::error::SchemaValidationError::OffSchemaState {
				state,
				allowed,
			} => Self::OffSchemaState { state, allowed },
			memory_kernel::error::SchemaValidationError::InvalidTransition {
				from,
				to,
				allowed_next,
				intermediate,
			} => Self::InvalidTransition {
				from,
				to,
				allowed_next,
				intermediate,
			},
			memory_kernel::error::SchemaValidationError::InvalidEdgeKind(
				kind,
			) => Self::InvalidEdgeKind(kind),
			memory_kernel::error::SchemaValidationError::RequiredStatesNotVisited {
				target,
				missing,
			} => Self::RequiredStatesNotVisited { target, missing },
		}
	}
}

impl From<memory_kernel::error::StorageSchemaError> for StorageSchemaError {
	fn from(value: memory_kernel::error::StorageSchemaError) -> Self {
		match value {
			memory_kernel::error::StorageSchemaError::VersionMismatch {
				found,
				expected,
			} => Self::VersionMismatch { found, expected },
		}
	}
}

impl From<memory_kernel::error::QueryParseError> for QueryParseError {
	fn from(value: memory_kernel::error::QueryParseError) -> Self {
		match value {
			memory_kernel::error::QueryParseError::InvalidExpression(expr) => {
				Self::InvalidExpression(expr)
			},
		}
	}
}

impl From<memory_kernel::error::StorageError> for StorageError {
	fn from(value: memory_kernel::error::StorageError) -> Self {
		match value {
			memory_kernel::error::StorageError::Database(e) => Self::Database(e),
			memory_kernel::error::StorageError::Io(e) => Self::Io(e),
			memory_kernel::error::StorageError::Serialization(e) => {
				Self::Serialization(e)
			},
			memory_kernel::error::StorageError::SchemaMismatch(e) => {
				Self::SchemaMismatch(e.into())
			},
			memory_kernel::error::StorageError::Validation(e) => {
				Self::Validation(e.into())
			},
			memory_kernel::error::StorageError::QueryParse(e) => {
				Self::QueryParse(e.into())
			},
			memory_kernel::error::StorageError::NotFound(id) => {
				Self::NotFound(id)
			},
			memory_kernel::error::StorageError::LeaseConflict {
				ticket,
				holder,
			} => Self::LeaseConflict { ticket, holder },
			memory_kernel::error::StorageError::WorkspaceNotFound { path } => {
				Self::WorkspaceNotFound { path }
			},
			memory_kernel::error::StorageError::DependencyCycle => {
				Self::DependencyCycle
			},
			memory_kernel::error::StorageError::DependencyNotProgressed {
				ticket,
				target_state,
				dependency,
				dependency_state,
			} => Self::DependencyNotProgressed {
				ticket,
				target_state,
				dependency,
				dependency_state,
			},
			memory_kernel::error::StorageError::SearchIndex(e) => {
				Self::SearchIndex(e)
			},
			memory_kernel::error::StorageError::ParseError { path, reason } => {
				Self::ParseError { path, reason }
			},
			memory_kernel::error::StorageError::SchemaFileParse {
				path,
				reason,
			} => Self::SchemaFileParse { path, reason },
			memory_kernel::error::StorageError::Protocol(e) => {
				Self::Other(format!("protocol: {e}"))
			},
			memory_kernel::error::StorageError::Other(e) => Self::Other(e),
		}
	}
}


impl From<memory_kernel::error::SchemaValidationError> for StorageError {
	fn from(value: memory_kernel::error::SchemaValidationError) -> Self {
		StorageError::Validation(value.into())
	}
}
