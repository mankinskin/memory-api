use std::collections::BTreeMap;

use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use std::path::Path;
use uuid::Uuid;

use ticket_api::{
    error::StorageError,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
    },
};

#[derive(Deserialize)]
pub struct WorkspaceParam {
    pub workspace: String,
    pub state: Option<String>,
    pub query: Option<String>,
    pub limit: Option<usize>,
    /// Pagination cursor — not yet implemented, accepted to keep the API forward-compatible.
    #[allow(dead_code)]
    pub cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct TicketIdParam {
    pub workspace: String,
}

#[derive(Clone, Serialize)]
pub struct TicketRef {
    pub workspace: String,
    pub id: String,
}

#[derive(Serialize)]
pub struct TicketSummary {
    pub id: String,
    pub ticket_ref: TicketRef,
    #[serde(rename = "type")]
    pub type_id: String,
    pub title: Option<String>,
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<u64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Serialize)]
pub struct TicketsResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub items: Vec<TicketSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct TicketDetailResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub ticket: TicketDetail,
}

#[derive(Serialize)]
pub struct TicketDetail {
    pub id: String,
    pub ticket_ref: TicketRef,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Serialize)]
pub struct TicketDescriptionResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub id: String,
    pub ticket_ref: TicketRef,
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct HistoryEntry {
    pub rev: u64,
    pub ts: String,
    pub author: Option<String>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Serialize)]
pub struct TicketHistoryResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub id: String,
    pub ticket_ref: TicketRef,
    pub count: u64,
    pub entries: Vec<HistoryEntry>,
}

#[derive(Deserialize)]
pub struct MutationWorkspaceParam {
    pub workspace: String,
}

#[derive(Deserialize)]
pub struct CreateTicketBody {
    #[serde(rename = "type")]
    pub type_id: String,
    pub title: Option<String>,
    pub fields: Option<BTreeMap<String, Value>>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTicketBody {
    pub fields: Option<BTreeMap<String, Value>>,
    pub state: Option<String>,
    #[serde(default)]
    pub transition_states: Vec<String>,
    pub description: Option<String>,
    /// How to apply `description`: `"replace"` (overwrites) or `"append"`
    /// (preserves existing content, concatenating onto it). Required
    /// (non-null) whenever `description` is set; there is no default.
    pub description_mode: Option<String>,
    /// Opt out of auto-walking multi-hop transitions. When true, a `state`
    /// that would skip a required waypoint is rejected with recovery guidance
    /// instead of traversing the intermediate states.
    #[serde(default)]
    pub single_hop: bool,
}

#[derive(Deserialize)]
pub struct CloseTicketBody {
    /// Target terminal state. Defaults to "done".
    pub target_state: Option<String>,
}

#[derive(Deserialize)]
pub struct CancelTicketBody {
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub struct RevertTicketBody {
    pub revision: u64,
}

#[derive(Deserialize)]
pub struct ReleaseLeaseBody {
    pub requester: String,
}

#[derive(Deserialize)]
pub struct MoveTicketBody {
    pub to_workspace_root: String,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize)]
pub struct MutationResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub ticket: TicketDetail,
}

#[derive(Serialize)]
pub struct DeleteResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub id: String,
    pub ticket_ref: TicketRef,
}

#[derive(Serialize)]
pub struct MoveTicketResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub id: String,
    pub status: String,
    pub mode: String,
    pub plan: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<serde_json::Value>,
    pub recovery: serde_json::Value,
}

#[derive(Serialize)]
pub struct ReleaseLeaseResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub id: String,
    pub ticket_ref: TicketRef,
    pub requester: String,
}

#[derive(Serialize)]
pub struct TicketFileEntry {
    /// Relative path within the ticket folder (e.g. "description.md" or
    /// "assets/design/plan.md").
    pub path: String,
    /// Display name — just the file's stem+extension (e.g. "plan.md").
    pub name: String,
}

#[derive(Serialize)]
pub struct TicketFilesResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub id: String,
    pub ticket_ref: TicketRef,
    pub files: Vec<TicketFileEntry>,
}

#[derive(Deserialize)]
pub struct TicketAssetParam {
    pub workspace: String,
    /// Relative path within the ticket folder, e.g. "assets/plan.md".
    pub path: String,
}

#[derive(Serialize)]
pub struct TicketAssetResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub id: String,
    pub ticket_ref: TicketRef,
    pub path: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct ListPartsParam {
    pub workspace: String,
    #[serde(default)]
    pub with_content: bool,
}

#[derive(Serialize)]
pub struct PartItem {
    pub id: Uuid,
    pub kind: String,
    pub path: String,
    pub frozen: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub supersedes: Option<Uuid>,
    pub implicit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Serialize)]
pub struct ListPartsResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub id: String,
    pub ticket_ref: TicketRef,
    pub count: usize,
    pub parts: Vec<PartItem>,
    pub orphans: Vec<String>,
}

#[derive(Serialize)]
pub struct PartResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub id: String,
    pub ticket_ref: TicketRef,
    pub part: PartItem,
}

#[derive(Deserialize)]
pub struct WritePartBody {
    /// Opaque part id (UUID) to update. Omit to create a new part.
    pub part_id: Option<Uuid>,
    /// Part kind. Used when creating a new part; ignored when updating an
    /// existing part.
    pub kind: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct WriteAmendmentBody {
    /// Opaque part id (UUID) of the part this amendment corrects.
    pub supersedes: Uuid,
    /// Opaque part id (UUID) for the new amendment part. Omit to generate one.
    pub part_id: Option<Uuid>,
    pub content: String,
}

pub fn ticket_ref_from_indexed(
    store: &TicketStore,
    active_workspace: &str,
    ticket: &IndexedTicket,
) -> Result<TicketRef, StorageError> {
    Ok(TicketRef {
        workspace: owning_workspace_for_path(
            store,
            active_workspace,
            &ticket.path,
        )?,
        id: ticket.id.to_string(),
    })
}

pub fn ticket_ref_for_id(
    store: &TicketStore,
    active_workspace: &str,
    id: &Uuid,
) -> Result<TicketRef, StorageError> {
    let indexed = store.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
    ticket_ref_from_indexed(store, active_workspace, &indexed)
}

fn owning_workspace_for_path(
    store: &TicketStore,
    active_workspace: &str,
    ticket_path: &Path,
) -> Result<String, StorageError> {
    let default_root = store.index_root.join("tickets");
    let mut best_label = active_workspace.to_string();
    let mut best_depth = if ticket_path.starts_with(&default_root) {
        default_root.components().count()
    } else {
        0
    };

    for root in store.list_scan_roots()? {
        if !ticket_path.starts_with(&root.path) {
            continue;
        }

        let depth = root.path.components().count();
        if depth > best_depth {
            best_depth = depth;
            best_label = root.label;
        }
    }

    Ok(best_label)
}
