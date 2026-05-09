use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Serialize)]
pub struct TicketSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub type_id: String,
    pub title: Option<String>,
    pub state: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Serialize)]
pub struct TicketsResponse {
    pub request_id: String,
    pub workspace: String,
    pub items: Vec<TicketSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct TicketDetailResponse {
    pub request_id: String,
    pub workspace: String,
    pub ticket: TicketDetail,
}

#[derive(Serialize)]
pub struct TicketDetail {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Serialize)]
pub struct TicketDescriptionResponse {
    pub request_id: String,
    pub workspace: String,
    pub id: String,
    pub description: Option<String>,
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
    pub from_state: Option<String>,
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
    pub workspace: String,
    pub ticket: TicketDetail,
}

#[derive(Serialize)]
pub struct DeleteResponse {
    pub request_id: String,
    pub workspace: String,
    pub id: String,
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
    pub workspace: String,
    pub id: String,
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
    pub workspace: String,
    pub id: String,
    pub path: String,
    pub content: String,
}