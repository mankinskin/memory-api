use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ticket::TicketId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GitHistoryMode {
    #[default]
    EmbeddedBare,
    WorkspaceGit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BranchLifecycle {
    pub created_on_branch: Option<String>,
    pub closed_on_branch: Option<String>,
    pub merge_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GitHistoryConfig {
    pub mode: GitHistoryMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub ticket_id: TicketId,
    pub commit_sha: String,
    pub actor: String,
    pub at: DateTime<Utc>,
    #[serde(default)]
    pub lifecycle: BranchLifecycle,
}
