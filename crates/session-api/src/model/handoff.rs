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

/// Handoff-package schema fields supplied by the caller to describe the next
/// implementation unit.  All fields are optional at the type level but the
/// store enforces required-field completeness when a package is provided.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHandoffPackage {
    /// The single goal of the next implementation unit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub objective: String,
    /// Ticket ids expected to be worked in the next session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_tickets: Vec<String>,
    /// Workspace-relative file paths expected to be touched.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_files: Vec<String>,
    /// Resolved design choices so the next session does not re-decide.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<String>,
    /// Explicit out-of-scope boundaries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_goals: Vec<String>,
    /// Prior findings, links, and ids needed so no search is required.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_anchors: Vec<String>,
    /// Must be empty for the package to be implementation-ready.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_escalations: Vec<String>,
    /// Known risks or fragile areas (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_notes: Option<String>,
    /// Id of the handoff this one supersedes (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_handoff: Option<String>,
}

impl SessionHandoffPackage {
    /// Returns `true` when all required fields are present and
    /// `open_escalations` is empty — i.e. the package is implementation-ready.
    pub fn is_implementation_ready(&self) -> bool {
        !self.objective.trim().is_empty()
            && self.open_escalations.is_empty()
            && !self.target_tickets.is_empty()
            && !self.target_files.is_empty()
            && !self.decisions.is_empty()
            && !self.non_goals.is_empty()
            && !self.context_anchors.is_empty()
    }

    /// Returns the names of required fields that are absent or empty.
    pub fn missing_fields(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.objective.trim().is_empty() {
            missing.push("objective");
        }
        if self.target_tickets.is_empty() {
            missing.push("target_tickets");
        }
        if self.target_files.is_empty() {
            missing.push("target_files");
        }
        if self.decisions.is_empty() {
            missing.push("decisions");
        }
        if self.non_goals.is_empty() {
            missing.push("non_goals");
        }
        if self.context_anchors.is_empty() {
            missing.push("context_anchors");
        }
        missing
    }
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
    // ── Handoff-package schema fields ────────────────────────────────────────
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub objective: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_tickets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_goals: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_anchors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_escalations: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_handoff: Option<String>,
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
