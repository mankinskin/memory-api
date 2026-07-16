use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use std::path::PathBuf;

pub const SESSION_SCHEMA_VERSION: u32 = 1;
pub const RUNTIME_CONTEXT_SCHEMA_VERSION: u32 = 1;

pub fn default_session_schema_version() -> u32 {
    SESSION_SCHEMA_VERSION
}

pub fn default_runtime_context_schema_version() -> u32 {
    RUNTIME_CONTEXT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionPinnedEntityKind {
    Ticket,
    Spec,
    Rule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPinnedEntity {
    pub urn: String,
    pub kind: SessionPinnedEntityKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub pinned_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRunLineage {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_session_id: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRuntimeContext {
    #[serde(default = "default_runtime_context_schema_version")]
    pub schema_version: u32,
    pub workspace_session_id: String,
    #[serde(default)]
    pub session_id: String,
    pub workspace_slug: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub active_run_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<SessionRunLineage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_entities: Vec<SessionPinnedEntity>,
    #[serde(default)]
    pub workflow: SessionWorkflowGraph,
}

impl SessionRuntimeContext {
    pub fn canonical_session_id(&self) -> String {
        if self.session_id.trim().is_empty() {
            self.workspace_session_id.clone()
        } else {
            self.session_id.clone()
        }
    }

    pub fn active_run(&self) -> Option<&SessionRunLineage> {
        self.runs
            .iter()
            .find(|run| run.run_id == self.active_run_id)
    }

    pub fn find_pin_mut(
        &mut self,
        urn: &str,
    ) -> Option<&mut SessionPinnedEntity> {
        self.pinned_entities.iter_mut().find(|pin| pin.urn == urn)
    }

    pub fn remove_pin(
        &mut self,
        urn: &str,
    ) -> bool {
        let before = self.pinned_entities.len();
        self.pinned_entities.retain(|pin| pin.urn != urn);
        self.pinned_entities.len() != before
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRuntimeInitRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_run_id: Option<String>,
    #[serde(default)]
    pub force_new_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRuntimeInitResult {
    pub context: SessionRuntimeContext,
    pub run: SessionRunLineage,
    pub created_workspace: bool,
    pub created_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPinnedEntityHeader {
    pub urn: String,
    pub kind: SessionPinnedEntityKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRuntimeView {
    pub workspace_session_id: String,
    pub active_run_id: String,
    pub pinned_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_headers: Vec<SessionPinnedEntityHeader>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionWorkflowNodeKind {
    Ticket,
    Action,
    Decision,
    Checkpoint,
    Validation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionWorkflowNodeRequirement {
    Required,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionWorkflowNodeStatus {
    Pending,
    InProgress,
    Blocked,
    Done,
    Deferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionWorkflowEdgeKind {
    DependsOn,
    Order,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkflowNode {
    pub node_id: String,
    pub kind: SessionWorkflowNodeKind,
    pub requirement: SessionWorkflowNodeRequirement,
    pub status: SessionWorkflowNodeStatus,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_urn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_ticket_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferred_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_spec_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkflowEdge {
    pub from: String,
    pub to: String,
    pub kind: SessionWorkflowEdgeKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkflowGraph {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<SessionWorkflowNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<SessionWorkflowEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkflowNodeDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub kind: SessionWorkflowNodeKind,
    pub requirement: SessionWorkflowNodeRequirement,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_urn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_ticket_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_spec_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkflowNodeResolution {
    pub node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_ticket_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkflowDiagnostic {
    pub node_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkflowSnapshot {
    pub workflow: SessionWorkflowGraph,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolutions: Vec<SessionWorkflowNodeResolution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<SessionWorkflowDiagnostic>,
}

pub trait SessionTicketStateResolver {
    fn resolve_ticket_state(
        &self,
        ticket_urn: &str,
    ) -> Result<Option<String>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionValidationGate {
    pub validation_spec_id: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

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

pub trait SessionPinFeedbackSink {
    fn record_pin_usage(
        &self,
        workspace_session_id: &str,
        run_id: &str,
        entity_urn: &str,
    ) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionLinks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ticket_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spec_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doc_evidence_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_run_id: Option<String>,
}

impl SessionLinks {
    pub fn links_to_ticket(
        &self,
        ticket_id: &str,
    ) -> bool {
        self.ticket_ids.iter().any(|id| id == ticket_id)
    }

    pub fn links_to_spec(
        &self,
        spec_id: &str,
    ) -> bool {
        self.spec_ids.iter().any(|id| id == spec_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTurnEventMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_success: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_requests_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_arguments_json: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTurn {
    pub sequence: usize,
    pub role: SessionRole,
    pub content: String,
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Active model that produced this turn, when known. `None` means the turn
    /// inherits the session-level model in [`SessionMetadata::model`]. This lets
    /// mid-session model routing (a large model delegating to cheaper ones) be
    /// observed at turn granularity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_meta: Option<SessionTurnEventMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub workspace_slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copilot_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vscode_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<SessionWorktreeAssignment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionWorktreeAllocationMode {
    New,
    Reused,
    Rotated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionWorktreeStatus {
    Active,
    Superseded,
    Invalidated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorktreeAssignment {
    pub path: PathBuf,
    pub branch: String,
    pub allocation_mode: SessionWorktreeAllocationMode,
    pub status: SessionWorktreeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    #[serde(default = "default_session_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub source: String,
    pub started_at: DateTime<Utc>,
    pub captured_at: DateTime<Utc>,
    pub metadata: SessionMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<SessionTurn>,
    #[serde(default)]
    pub links: SessionLinks,
}

impl SessionRecord {
    pub fn has_turns(&self) -> bool {
        !self.turns.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    use super::{
        SessionLinks,
        SessionMetadata,
        SessionRecord,
        SessionRole,
        SessionTurn,
        SessionTurnEventMeta,
        SessionWorktreeAllocationMode,
        SessionWorktreeAssignment,
        SessionWorktreeStatus,
    };
    use crate::SESSION_SCHEMA_VERSION;

    fn sample_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 12, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn session_record_round_trips_through_serde() {
        let record = SessionRecord {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id: "session-123".to_string(),
            source: "copilot-hook".to_string(),
            started_at: sample_time(),
            captured_at: sample_time(),
            metadata: SessionMetadata {
                workspace_slug: "context-engine".to_string(),
                conversation_id: Some("conversation-1".to_string()),
                agent_id: Some("github-copilot-gpt-5.4".to_string()),
                ticket_id: Some("ticket-1".to_string()),
                model: Some("GPT-5.4".to_string()),
                trigger: Some("post-turn".to_string()),
                producer: Some("copilot-agent".to_string()),
                copilot_version: Some("0.55.0".to_string()),
                vscode_version: Some("1.127.0".to_string()),
                protocol_version: Some(1),
                worktree: Some(SessionWorktreeAssignment {
                    path: PathBuf::from("worktrees/session-123"),
                    branch: "session/session-123".to_string(),
                    allocation_mode: SessionWorktreeAllocationMode::New,
                    status: SessionWorktreeStatus::Active,
                    predecessor_session_id: None,
                    predecessor_path: None,
                }),
            },
            turns: vec![SessionTurn {
                sequence: 0,
                role: SessionRole::User,
                content: "Summarize the test failures".to_string(),
                captured_at: sample_time(),
                tool_name: None,
                model: Some("GPT-5.4".to_string()),
                event_meta: Some(SessionTurnEventMeta {
                    event_id: Some("evt-1".to_string()),
                    parent_event_id: None,
                    event_type: Some("user.message".to_string()),
                    turn_id: Some("turn-1".to_string()),
                    message_id: Some("msg-1".to_string()),
                    tool_call_id: None,
                    tool_success: None,
                    reasoning_text: None,
                    tool_requests_json: None,
                    tool_arguments_json: None,
                }),
            }],
            links: SessionLinks {
                ticket_ids: vec!["ticket-1".to_string()],
                spec_ids: vec!["spec-1".to_string()],
                doc_evidence_ids: vec!["doc-1".to_string()],
                log_ids: vec!["log-1".to_string()],
                runtime_session_id: Some(
                    "03baab6c-0fdb-4ffc-8159-b83066a6283f".to_string(),
                ),
                runtime_run_id: Some(
                    "8cf1255d-7969-4ac2-905a-cbd234dc3eac".to_string(),
                ),
            },
        };

        let json = serde_json::to_string_pretty(&record).unwrap();
        let reparsed: SessionRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(reparsed, record);
        assert_eq!(
            record.turns[0].model.as_deref(),
            Some("GPT-5.4"),
            "per-turn model should round-trip"
        );
        assert!(record.links.links_to_ticket("ticket-1"));
        assert!(record.links.links_to_spec("spec-1"));
    }
}
