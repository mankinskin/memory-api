use std::collections::BTreeMap;

use rmcp::schemars::{
    self,
    JsonSchema,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;

#[derive(Serialize)]
pub struct TicketSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub type_id: String,
    pub title: Option<String>,
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<u64>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct TicketDetail {
    pub id: String,
    pub path: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Clone, Serialize)]
pub struct EdgeItem {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Serialize)]
pub struct NodeItem {
    pub id: String,
    pub title: Option<String>,
    pub state: Option<String>,
    pub depth: usize,
}

#[derive(Serialize)]
pub struct SubgraphResponse {
    pub workspace: String,
    pub nodes: Vec<NodeItem>,
    pub edges: Vec<EdgeItem>,
    pub truncated: bool,
    pub stats: SubgraphStats,
}

#[derive(Serialize)]
pub struct SubgraphStats {
    pub nodes_returned: usize,
    pub edges_returned: usize,
    pub max_depth_reached: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTicketsInput {
    pub workspace: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default, rename = "type")]
    pub type_id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TicketRefInput {
    #[serde(default)]
    pub workspace: Option<String>,
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListEdgesInput {
    pub workspace: String,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SubgraphInput {
    pub workspace: String,
    pub root: String,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub edge_kind: Option<String>,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
    pub limit_nodes: Option<usize>,
    #[serde(default)]
    pub limit_edges: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopgraphInput {
    pub workspace: String,
    pub root: String,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub edge_kind: Option<String>,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
    pub limit_nodes: Option<usize>,
    #[serde(default)]
    pub limit_edges: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HealthCheckInput {
    pub workspace: String,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub r#where: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowName {
    List,
    TriageOpenTickets,
    FetchTicketContext,
    InspectDependencies,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateTicketInput {
    pub workspace: String,
    pub id: String,
    #[serde(default)]
    pub transition_states: Vec<String>,
    #[serde(default)]
    pub to_state: Option<String>,
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    #[serde(default)]
    pub field_map: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub undo: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CloseTicketInput {
    pub workspace: String,
    pub id: String,
    #[serde(default = "default_close_state")]
    pub to_state: String,
    #[serde(default)]
    pub author: Option<String>,
}

fn default_close_state() -> String {
    "done".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CancelTicketInput {
    pub workspace: String,
    pub id: String,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateTicketInput {
    pub workspace: String,
    #[serde(rename = "type")]
    pub type_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteTicketInput {
    pub workspace: String,
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddEdgeInput {
    pub workspace: String,
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RemoveEdgeInput {
    pub workspace: String,
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DanglingStrategy {
    Unlink,
    ReconcileOnly,
}

impl DanglingStrategy {
    pub fn mutates(&self) -> bool {
        matches!(self, Self::Unlink)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unlink => "unlink",
            Self::ReconcileOnly => "reconcile_only",
        }
    }
}

fn default_dangling_kind() -> String {
    "depends_on".to_string()
}

fn default_dangling_strategy() -> DanglingStrategy {
    DanglingStrategy::Unlink
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PruneDanglingEdgesInput {
    pub workspace: String,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub all: bool,
    #[serde(default = "default_dangling_kind")]
    pub kind: String,
    #[serde(default = "default_dangling_strategy")]
    pub strategy: DanglingStrategy,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowInput {
    #[serde(default = "default_workflow_name")]
    pub name: WorkflowName,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NextTicketsInput {
    pub workspace: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub filter: Option<String>,
    /// Optional ticket UUID or 8+ character hex prefix.
    /// When set, scope results to actionable leaf blockers beneath this ticket.
    #[serde(default)]
    pub root: Option<String>,
}

fn default_workflow_name() -> WorkflowName {
    WorkflowName::List
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardShowInput {
    pub workspace: String,
    #[serde(default)]
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardHistoryInput {
    pub workspace: String,
    #[serde(default)]
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardCheckInInput {
    pub workspace: String,
    pub ticket_id: String,
    pub agent_id: String,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub ttl_secs: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardCheckOutInput {
    pub workspace: String,
    pub ticket_id: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardReleaseLeaseInput {
    pub workspace: String,
    pub ticket_id: String,
    pub requester: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardHeartbeatInput {
    pub workspace: String,
    pub entry_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardConfigureInput {
    pub workspace: String,
    #[serde(default)]
    pub max_wip: Option<u32>,
    #[serde(default)]
    pub stale_after_secs: Option<u64>,
    #[serde(default)]
    pub completed_audit_window_secs: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardCleanPreviewInput {
    pub workspace: String,
    #[serde(default)]
    pub include_stale: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardCleanApplyInput {
    pub workspace: String,
    pub token: String,
    #[serde(default)]
    pub include_stale: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardUpdateFilesInput {
    pub workspace: String,
    pub ticket_id: String,
    pub agent_id: String,
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardRenameFileInput {
    pub workspace: String,
    pub ticket_id: String,
    pub agent_id: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MovePreflightInput {
    pub workspace: String,
    pub id: String,
    pub to_workspace_root: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveApplyInput {
    pub workspace: String,
    pub id: String,
    pub to_workspace_root: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveJournalInput {
    pub workspace: String,
    pub id: String,
}
