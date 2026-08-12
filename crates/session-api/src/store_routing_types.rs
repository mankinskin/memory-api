use super::*;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStorePaths {
    pub session_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub transcript_path: PathBuf,
    pub events_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRuntimePaths {
    pub workspace_dir: PathBuf,
    pub context_path: PathBuf,
    pub handoffs_dir: PathBuf,
    pub finish_path: PathBuf,
}

pub(super) fn validate_runtime_workspace_id(
    value: &str
) -> Result<(), SessionError> {
    validate_session_identity(value)
}

pub(super) fn validate_session_identity(
    value: &str,
) -> Result<(), SessionError> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| SessionError::SessionIdentityMustBeUuid(value.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedEntityUrn {
    pub(super) workspace_slug: String,
    pub(super) kind: SessionPinnedEntityKind,
    pub(super) entity_id: String,
}

pub(super) fn parse_entity_urn(
    entity_urn: &str
) -> Result<ParsedEntityUrn, SessionError> {
    let trimmed = entity_urn.trim();
    if !trimmed.starts_with("ce://") {
        return Err(SessionError::InvalidEntityUrn(trimmed.to_string()));
    }

    let rest = trimmed.trim_start_matches("ce://");
    let mut segments = rest.split('/');
    let workspace_slug = segments.next().unwrap_or_default().to_string();
    let store = segments.next().unwrap_or_default();
    let entity_id = segments.next().unwrap_or_default().to_string();
    if workspace_slug.trim().is_empty() || entity_id.trim().is_empty() {
        return Err(SessionError::InvalidEntityUrn(trimmed.to_string()));
    }
    if segments.next().is_some() {
        return Err(SessionError::InvalidEntityUrn(trimmed.to_string()));
    }

    let kind = match store {
        "ticket" | "tickets" => SessionPinnedEntityKind::Ticket,
        "spec" | "specs" => SessionPinnedEntityKind::Spec,
        "rule" | "rules" => SessionPinnedEntityKind::Rule,
        _ => return Err(SessionError::InvalidEntityUrn(trimmed.to_string())),
    };

    Ok(ParsedEntityUrn {
        workspace_slug,
        kind,
        entity_id,
    })
}

pub(super) fn parse_entity_urn_kind(
    entity_urn: &str
) -> Result<SessionPinnedEntityKind, SessionError> {
    Ok(parse_entity_urn(entity_urn)?.kind)
}
