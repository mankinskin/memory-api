use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    SessionPinnedEntityHeader,
    SessionValidationGate,
    SessionWorkflowSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHandoffRecord {
    pub handoff_id: String,
    pub workspace_session_id: String,
    pub outgoing_run_id: String,
    pub created_at: DateTime<Utc>,
    pub resume_command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_entities: Vec<SessionPinnedEntityHeader>,
    pub workflow: SessionWorkflowSnapshot,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation: Vec<SessionValidationGate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHandoffResult {
    pub record: SessionHandoffRecord,
    pub record_path: String,
    pub render: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionFinishRecord {
    pub workspace_session_id: String,
    pub run_id: String,
    pub finished_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deferred_optional_node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation: Vec<SessionValidationGate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionFinishResult {
    pub record: SessionFinishRecord,
    pub already_finished: bool,
}
