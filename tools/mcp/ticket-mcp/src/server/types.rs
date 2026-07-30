use std::{
    borrow::Cow,
    collections::BTreeMap,
};

use rmcp::schemars::{
    self,
    JsonSchema,
    Schema,
    SchemaGenerator,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use ticket_api::storage::DescriptionUpdate;

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
    /// Named read-projection view profile: summary, plan, review, or full.
    /// Mutually exclusive with `parts`. Defaults to `summary` when neither
    /// is supplied. Ignored by tools other than `get_ticket`.
    #[serde(default)]
    pub view: Option<String>,
    /// Explicit comma-separated part-kind list to project (e.g.
    /// "objective,acceptance_criteria"). Mutually exclusive with `view`.
    /// Ignored by tools other than `get_ticket`.
    #[serde(default)]
    pub parts: Option<String>,
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

// Wire shape for the `update_ticket` MCP tool: the raw `description` +
// `description_mode` pair as received over JSON. Drives both deserialization
// and the advertised JSON Schema for `UpdateTicketInput` so the tool's schema
// is unchanged; never used past decoding into a single `DescriptionUpdate` so
// "content without a mode" cannot be constructed downstream (AC5 of ticket
// 3d952036). Not a doc comment: schemars would otherwise fold this into the
// root schema's `description`, changing the advertised schema.

#[derive(Debug, Deserialize, JsonSchema)]
struct UpdateTicketInputWire {
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
    /// How to apply `description`: `"replace"` (overwrites) or `"append"`
    /// (preserves existing content, concatenating onto it). Required
    /// whenever `description` is set — there is no default, and omitting it
    /// is rejected rather than silently applying `replace`.
    #[serde(default)]
    pub description_mode: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// Opt out of auto-walking multi-hop transitions. When true, a `to_state`
    /// that would skip a required waypoint is rejected with recovery guidance
    /// instead of traversing the intermediate states.
    #[serde(default)]
    pub single_hop: bool,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "UpdateTicketInputWire")]
pub struct UpdateTicketInput {
    pub workspace: String,
    pub id: String,
    pub transition_states: Vec<String>,
    pub to_state: Option<String>,
    pub fields: Option<Vec<String>>,
    pub field_map: Option<BTreeMap<String, Value>>,
    pub undo: bool,
    /// How `description` (if any) applies to `description.md`. There is no
    /// separate `description_mode` field: [`DescriptionUpdate::Unchanged`]
    /// means no description change, and content only exists paired with an
    /// explicit mode (AC5 of ticket 3d952036).
    pub description_update: DescriptionUpdate,
    pub author: Option<String>,
    pub single_hop: bool,
}

impl TryFrom<UpdateTicketInputWire> for UpdateTicketInput {
    type Error = String;

    fn try_from(wire: UpdateTicketInputWire) -> Result<Self, Self::Error> {
        let description_update =
            DescriptionUpdate::decode(wire.description, wire.description_mode.as_deref())?;
        Ok(UpdateTicketInput {
            workspace: wire.workspace,
            id: wire.id,
            transition_states: wire.transition_states,
            to_state: wire.to_state,
            fields: wire.fields,
            field_map: wire.field_map,
            undo: wire.undo,
            description_update,
            author: wire.author,
            single_hop: wire.single_hop,
        })
    }
}

// Field layout, docs, and required set delegate to `UpdateTicketInputWire` so
// the advertised MCP tool schema keeps the `description` / `description_mode`
// JSON fields exactly as before this type stopped deriving `JsonSchema`
// directly. `schema_name`/`schema_id` are NOT delegated: `root_schema_for`
// uses `T::schema_name()` (the outer type, i.e. `UpdateTicketInput`) for the
// root `"title"`, so these are set to the literal values the original
// `#[derive(JsonSchema)]` on `UpdateTicketInput` itself would have produced.
impl JsonSchema for UpdateTicketInput {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("UpdateTicketInput")
    }

    fn schema_id() -> Cow<'static, str> {
        Cow::Borrowed(concat!(module_path!(), "::UpdateTicketInput"))
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        UpdateTicketInputWire::json_schema(generator)
    }

    fn inline_schema() -> bool {
        false
    }
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListPartsInput {
    pub workspace: String,
    pub id: String,
    #[serde(default)]
    pub with_content: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPartInput {
    pub workspace: String,
    pub id: String,
    pub part_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WritePartInput {
    pub workspace: String,
    pub id: String,
    /// Opaque part id (UUID) to update. Omit to create a new part.
    #[serde(default)]
    pub part_id: Option<String>,
    /// Part kind (e.g. objective, requirements, review, or any free-form
    /// attachment kind). Used when creating a new part; ignored when
    /// updating an existing part.
    pub kind: String,
    pub content: String,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteAmendmentInput {
    pub workspace: String,
    pub id: String,
    /// Opaque part id (UUID) of the part this amendment corrects.
    pub supersedes: String,
    /// Opaque part id (UUID) for the new amendment part. Omit to generate one.
    #[serde(default)]
    pub part_id: Option<String>,
    pub content: String,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UndoPartInput {
    pub workspace: String,
    pub id: String,
    pub part_id: String,
    #[serde(default)]
    pub author: Option<String>,
}
