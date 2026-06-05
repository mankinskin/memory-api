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

use crate::serve::registry::{
    canonical_workspace_name_for_index_root,
    store_root_for_scan_root,
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
            best_label = store_root_for_scan_root(&root.path)
                .map(|index_root| {
                    canonical_workspace_name_for_index_root(
                        &index_root,
                        &root.label,
                    )
                })
                .unwrap_or(root.label);
        }
    }

    Ok(best_label)
}
