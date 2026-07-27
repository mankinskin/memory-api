use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

/// Behavioral role of a workflow node.
///
/// This axis is a **closed, validated set** because production finish/handoff
/// logic branches on it and each behavioral kind carries required side-data:
///
/// - `Ticket` gates finish on authoritative live ticket state and carries a
///   `ticket_urn`.
/// - `Validation` gates finish on authoritative validation execution outcomes
///   and carries a `validation_spec_id`.
/// - `Spec` gates finish on authoritative live spec state and carries a
///   `spec_urn` (symmetric to `Ticket`).
/// - `Task` is the generic non-gating bucket for descriptive work. It never
///   drives finish behavior; descriptive nuance belongs in the open
///   [`SessionWorkflowNode::category`] free-text field or the node `title`.
///
/// The legacy cosmetic kinds `action`, `decision`, and `checkpoint` branched in
/// no production code. They are accepted on deserialize as aliases of `Task`
/// so existing persisted runtime contexts continue to load, and are re-emitted
/// as `task`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionWorkflowNodeKind {
    /// Ticket-backed behavioral node; gates finish on live ticket state.
    Ticket,
    /// Validation behavioral node; gates finish on authoritative execution.
    Validation,
    /// Spec-backed behavioral node; gates finish on live spec state.
    Spec,
    /// Generic descriptive node; never gates finish. Accepts the deprecated
    /// `action`/`decision`/`checkpoint` kinds as back-compat aliases.
    #[serde(alias = "action", alias = "decision", alias = "checkpoint")]
    Task,
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
    pub spec_urn: Option<String>,
    /// Optional ticket or spec reference for context. Unlike `ticket_urn` and
    /// `spec_urn`, this field never participates in finish gating.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_urn: Option<String>,
    /// Open, free-text descriptive classification. No production code branches
    /// on this value; it exists so agents never hit an expressiveness wall for
    /// labels that do not drive behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
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
    pub spec_urn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_urn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
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

    /// Resolve the authoritative live state for a spec-backed workflow node.
    ///
    /// Mirrors [`Self::resolve_ticket_state`] so a required `Spec` node can gate
    /// finish symmetrically to a `Ticket` node. The default implementation
    /// reports the capability as unavailable, which fails a required `Spec` node
    /// closed rather than silently passing it.
    fn resolve_spec_state(
        &self,
        spec_urn: &str,
    ) -> Result<Option<String>, String> {
        Err(format!(
            "spec state resolution not supported by this resolver ({spec_urn})"
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionValidationGate {
    pub validation_spec_id: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// The command that performs the validation check. Optional; when absent,
    /// `validation_spec_id` should reference a test-api ValidationSpec entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}
