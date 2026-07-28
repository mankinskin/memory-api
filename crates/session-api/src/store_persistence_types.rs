use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSessionManifest {
    #[serde(default = "crate::default_session_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub source: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    pub metadata: SessionMetadata,
    #[serde(default)]
    pub links: SessionLinks,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_ticket_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawned_session_id: Option<String>,
}

impl From<&SessionRecord> for PersistedSessionManifest {
    fn from(record: &SessionRecord) -> Self {
        Self {
            schema_version: record.schema_version,
            session_id: record.session_id.clone(),
            source: record.source.clone(),
            started_at: record.started_at,
            captured_at: record.captured_at,
            metadata: record.metadata.clone(),
            links: record.links.clone(),
            track_id: record.track_id.clone(),
            anchor_ticket_id: record.anchor_ticket_id.clone(),
            parent_session_id: record.parent_session_id.clone(),
            spawned_session_id: record.spawned_session_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedSessionTranscript {
    #[serde(default = "crate::default_session_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<SessionTurn>,
}

impl From<&SessionRecord> for PersistedSessionTranscript {
    fn from(record: &SessionRecord) -> Self {
        Self {
            schema_version: record.schema_version,
            session_id: record.session_id.clone(),
            captured_at: record.captured_at,
            turns: record.turns.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSessionEvents {
    #[serde(default = "crate::default_session_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<CopilotHookEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedActiveWorkspaceSession {
    pub workspace_session_id: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedRuntimeContext {
    #[serde(default = "crate::default_runtime_context_schema_version")]
    pub schema_version: u32,
    pub workspace_session_id: String,
    pub workspace_slug: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub active_run_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<SessionRunLineage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_entities: Vec<SessionPinnedEntity>,
    #[serde(default)]
    pub workflow: crate::SessionWorkflowGraph,
}

impl From<SessionRuntimeContext> for PersistedRuntimeContext {
    fn from(value: SessionRuntimeContext) -> Self {
        Self {
            schema_version: value.schema_version,
            workspace_session_id: value.workspace_session_id,
            workspace_slug: value.workspace_slug,
            created_at: value.created_at,
            updated_at: value.updated_at,
            active_run_id: value.active_run_id,
            runs: value.runs,
            pinned_entities: value.pinned_entities,
            workflow: value.workflow,
        }
    }
}

impl From<PersistedRuntimeContext> for SessionRuntimeContext {
    fn from(value: PersistedRuntimeContext) -> Self {
        Self {
            schema_version: value.schema_version,
            workspace_session_id: value.workspace_session_id.clone(),
            session_id: value.workspace_session_id,
            workspace_slug: value.workspace_slug,
            created_at: value.created_at,
            updated_at: value.updated_at,
            active_run_id: value.active_run_id,
            runs: value.runs,
            pinned_entities: value.pinned_entities,
            workflow: value.workflow,
        }
    }
}
