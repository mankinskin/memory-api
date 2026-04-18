use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Metadata stored per-entity in the redb index.
/// Does not hold full content — that lives in the manifest file on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedEntity {
    pub id: Uuid,
    /// Absolute path to the entity folder.
    pub path: PathBuf,
    pub type_id: String,
    pub title: Option<String>,
    pub state: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Soft-delete flag. Deleted entities are kept in the index for audit.
    pub deleted: bool,
}

/// Lease record stored in the LEASES redb table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseInfo {
    pub ticket_id: Uuid,
    /// Identity of the agent holding the lease (e.g. agent name or session ID).
    pub working_by: String,
    /// Optional description of what the worker intends to do.
    pub work_intent: Option<String>,
    pub claimed_at: DateTime<Utc>,
    pub lease_expires_at: DateTime<Utc>,
    /// TTL in seconds used to renew heartbeats.
    pub ttl_secs: u64,
    /// Optional mutual-exclusion group tag.
    pub conflict_domain: Option<String>,
}

impl LeaseInfo {
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.lease_expires_at
    }
}
