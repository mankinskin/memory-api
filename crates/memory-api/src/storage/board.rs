use std::collections::BTreeMap;

use chrono::{
    DateTime,
    Duration,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};
use uuid::Uuid;

use crate::error::StorageError;

const BOARD_CONFIG_KEY: &str = "default";

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardEntry {
    pub entry_id: Uuid,
    pub ticket_id: Uuid,
    pub agent_id: String,
    pub previous_attempt: Option<Uuid>,
    pub checked_in_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub ttl_secs: u64,
    pub intent: String,
    pub owned_files: Vec<String>,
    pub status: BoardEntryStatus,
    /// Populated on check-out; not persisted during check-in.
    pub handoff_reason: Option<String>,
    /// When the entry left the active board and became historical.
    pub completed_at: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
}

impl BoardEntry {
    /// Returns `true` if this entry would be considered stale at the given time.
    ///
    /// Stale means the entry is `Active` but the heartbeat has expired.
    /// This is computed dynamically and is **not** written back to storage.
    pub fn is_stale_at(
        &self,
        now: DateTime<Utc>,
    ) -> bool {
        self.status == BoardEntryStatus::Active
            && now
                > self.last_heartbeat + Duration::seconds(self.ttl_secs as i64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BoardEntryStatus {
    Active,
    /// Computed dynamically in snapshots; `Active` entries whose heartbeat TTL
    /// has elapsed appear as `Stale` in [`BoardSnapshot`] but are stored as
    /// `Active` in the database.
    Stale,
    /// Marked when a conflicting check-in detects file ownership overlap.
    Conflict,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardConfig {
    pub max_wip: u32,
    pub stale_after_secs: u64,
    pub completed_audit_window_secs: u64,
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self {
            max_wip: 5,
            stale_after_secs: 3600,
            completed_audit_window_secs: 3600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardSnapshot {
    pub captured_at: DateTime<Utc>,
    /// Current board entries (with stale status computed dynamically).
    pub entries: Vec<BoardEntry>,
    /// Filtered to the requesting agent's entries when `agent_id` is `Some`.
    pub caller_entries: Vec<BoardEntry>,
    pub config: BoardConfig,
    pub active_count: u32,
    pub stale_count: u32,
    pub conflict_count: u32,
    /// `true` when `active_count + stale_count >= config.max_wip`.
    pub wip_limit_reached: bool,
    /// Maps each owned file path to the list of agent IDs holding it.
    pub file_ownership: BTreeMap<String, Vec<String>>,
    pub active_worktrees: Vec<ActiveWorktree>,
    /// Human-readable warnings (e.g. stale entries needing review).
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveWorktree {
    pub worktree_path: String,
    pub branch: Option<String>,
    pub session_ids: Vec<String>,
    pub agent_ids: Vec<String>,
    pub ticket_ids: Vec<Uuid>,
    pub entry_ids: Vec<Uuid>,
    /// True when more than one distinct session id claims this worktree.
    pub conflicted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardHistorySnapshot {
    pub captured_at: DateTime<Utc>,
    /// Recently completed historical entries, newest completion first.
    pub entries: Vec<BoardEntry>,
    /// Filtered to the requesting agent's entries when `agent_id` is `Some`.
    pub caller_entries: Vec<BoardEntry>,
    pub config: BoardConfig,
    pub completed_count: u32,
    pub hidden_completed_count: u32,
}

// ── Operational maintenance types ─────────────────────────────────────────────

/// Preview of entries that are eligible for removal by `board_clean_apply`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardCleanPreview {
    pub generated_at: DateTime<Utc>,
    /// Stateless verification token (opaque, SHA-256 based).
    ///
    /// Pass this value back verbatim to `board_clean_apply`.  The server
    /// re-derives the set of eligible entries and verifies the token; if the
    /// board has changed in the interim the call is rejected with
    /// [`BoardError::StaleCleanToken`].
    pub token: String,
    /// IDs of the entries that will be deleted when the token is applied.
    pub entry_ids: Vec<Uuid>,
    pub entry_count: usize,
    pub include_stale: bool,
}

/// Outcome of a successful `board_clean_apply` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardCleanResult {
    pub removed_entry_ids: Vec<Uuid>,
    pub removed_count: usize,
}

/// Action taken by `board_reconcile` for a given ticket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReconcileAction {
    /// An active board entry was found and marked `Completed` because the
    /// ticket reached a terminal state.
    MarkedCompleted { entry_id: Uuid },
    /// The ticket was reverted while an active board entry exists.  The entry
    /// remains active; a warning is emitted at the call site.
    StaleIntentWarning {
        entry_id: Uuid,
        current_state: String,
    },
    /// No active board entry was found for this ticket.
    NoEntry,
}

/// Result returned by the internal `board_reconcile` helper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardReconcileResult {
    pub ticket_id: Uuid,
    pub action: ReconcileAction,
}

#[derive(Debug, thiserror::Error)]
pub enum BoardError {
    #[error("WIP limit reached: {current}/{max} active entries")]
    WipLimitReached { current: u32, max: u32 },
    #[error(
        "File conflict on {files:?} with agent {conflicting_agent} (ticket {conflicting_ticket})"
    )]
    FileConflict {
        files: Vec<String>,
        conflicting_agent: String,
        conflicting_ticket: Uuid,
    },
    #[error(
        "Worktree conflict on '{worktree_path}' with agent {conflicting_agent} (ticket {conflicting_ticket})"
    )]
    WorktreeConflict {
        worktree_path: String,
        conflicting_agent: String,
        conflicting_ticket: Uuid,
    },
    #[error("Already checked in: ticket {ticket_id} by {agent_id}")]
    AlreadyCheckedIn { ticket_id: Uuid, agent_id: String },
    #[error("Not checked in: ticket {ticket_id} by {agent_id}")]
    NotCheckedIn { ticket_id: Uuid, agent_id: String },
    #[error("Ticket not found: {0}")]
    TicketNotFound(Uuid),
    #[error("Entry not found: {0}")]
    EntryNotFound(Uuid),
    #[error(
        "clean token is stale: board has changed since the preview was generated"
    )]
    StaleCleanToken,
    #[error(
        "file rename conflict: '{path}' is owned by agent {conflicting_agent} (ticket {conflicting_ticket})"
    )]
    FileRenameConflict {
        path: String,
        conflicting_agent: String,
        conflicting_ticket: Uuid,
    },
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

// ── Token helpers ─────────────────────────────────────────────────────────────

/// Compute the opaque clean token from a sorted list of entry IDs and the
/// timestamp at which the preview was generated.
///
/// Token format: `"{sha256_hex}|{generated_at_millis}"`.
fn compute_clean_token(
    sorted_ids: &[Uuid],
    generated_at: DateTime<Utc>,
) -> String {
    let ts_millis = generated_at.timestamp_millis();
    let mut hasher = Sha256::new();
    for id in sorted_ids {
        hasher.update(id.as_bytes());
    }
    hasher.update(ts_millis.to_le_bytes());
    let hash = hasher.finalize();
    let hash_hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("{hash_hex}|{ts_millis}")
}

fn parse_clean_token(
    token: &str
) -> Result<(String, DateTime<Utc>), BoardError> {
    let Some((hash_hex, millis_str)) = token.split_once('|') else {
        return Err(BoardError::StaleCleanToken);
    };
    let ts_millis: i64 = millis_str
        .parse()
        .map_err(|_| BoardError::StaleCleanToken)?;
    let generated_at = DateTime::from_timestamp_millis(ts_millis)
        .ok_or(BoardError::StaleCleanToken)?;
    Ok((hash_hex.to_string(), generated_at))
}

// ── Serde helpers ─────────────────────────────────────────────────────────────

fn serialize_entry(entry: &BoardEntry) -> Result<Vec<u8>, BoardError> {
    bincode::serialize(entry).map_err(|e| {
        BoardError::Storage(StorageError::Serialization(e.to_string()))
    })
}

fn deserialize_entry(bytes: &[u8]) -> Result<BoardEntry, BoardError> {
    // Order is load-bearing: shorter shapes can parse a longer payload prefix.
    bincode::deserialize(bytes)
        .or_else(|_| {
            bincode::deserialize::<BoardEntryV2>(bytes).map(Into::into)
        })
        .or_else(|_| {
            bincode::deserialize::<BoardEntryV1>(bytes).map(Into::into)
        })
        .map_err(|e| {
            BoardError::Storage(StorageError::Serialization(e.to_string()))
        })
}

fn serialize_config(config: &BoardConfig) -> Result<Vec<u8>, BoardError> {
    bincode::serialize(config).map_err(|e| {
        BoardError::Storage(StorageError::Serialization(e.to_string()))
    })
}

fn deserialize_config(bytes: &[u8]) -> Result<BoardConfig, BoardError> {
    bincode::deserialize(bytes).map_err(|e| {
        BoardError::Storage(StorageError::Serialization(e.to_string()))
    })
}

fn db_err(e: rusqlite::Error) -> BoardError {
    BoardError::Storage(StorageError::Database(e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BoardEntryV1 {
    entry_id: Uuid,
    ticket_id: Uuid,
    agent_id: String,
    previous_attempt: Option<Uuid>,
    checked_in_at: DateTime<Utc>,
    last_heartbeat: DateTime<Utc>,
    ttl_secs: u64,
    intent: String,
    owned_files: Vec<String>,
    status: BoardEntryStatus,
    handoff_reason: Option<String>,
}

impl From<BoardEntryV1> for BoardEntry {
    fn from(entry: BoardEntryV1) -> Self {
        Self {
            entry_id: entry.entry_id,
            ticket_id: entry.ticket_id,
            agent_id: entry.agent_id,
            previous_attempt: entry.previous_attempt,
            checked_in_at: entry.checked_in_at,
            last_heartbeat: entry.last_heartbeat,
            ttl_secs: entry.ttl_secs,
            intent: entry.intent,
            owned_files: entry.owned_files,
            status: entry.status,
            handoff_reason: entry.handoff_reason,
            completed_at: None,
            session_id: None,
            worktree_path: None,
            branch: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BoardEntryV2 {
    entry_id: Uuid,
    ticket_id: Uuid,
    agent_id: String,
    previous_attempt: Option<Uuid>,
    checked_in_at: DateTime<Utc>,
    last_heartbeat: DateTime<Utc>,
    ttl_secs: u64,
    intent: String,
    owned_files: Vec<String>,
    status: BoardEntryStatus,
    handoff_reason: Option<String>,
    completed_at: Option<DateTime<Utc>>,
}

impl From<BoardEntryV2> for BoardEntry {
    fn from(entry: BoardEntryV2) -> Self {
        Self {
            entry_id: entry.entry_id,
            ticket_id: entry.ticket_id,
            agent_id: entry.agent_id,
            previous_attempt: entry.previous_attempt,
            checked_in_at: entry.checked_in_at,
            last_heartbeat: entry.last_heartbeat,
            ttl_secs: entry.ttl_secs,
            intent: entry.intent,
            owned_files: entry.owned_files,
            status: entry.status,
            handoff_reason: entry.handoff_reason,
            completed_at: entry.completed_at,
            session_id: None,
            worktree_path: None,
            branch: None,
        }
    }
}

mod ops;

#[cfg(test)]
mod tests {
    use super::*;

    fn v1_entry() -> BoardEntryV1 {
        BoardEntryV1 {
            entry_id: Uuid::new_v4(),
            ticket_id: Uuid::new_v4(),
            agent_id: "agent".to_string(),
            previous_attempt: None,
            checked_in_at: Utc::now(),
            last_heartbeat: Utc::now(),
            ttl_secs: 60,
            intent: "test".to_string(),
            owned_files: Vec::new(),
            status: BoardEntryStatus::Active,
            handoff_reason: None,
        }
    }

    #[test]
    fn deserialize_entry_loads_v1_and_v2_without_worktree_metadata() {
        let v1 = v1_entry();
        let v1_bytes = bincode::serialize(&v1).unwrap();
        let loaded_v1 = deserialize_entry(&v1_bytes).unwrap();
        assert!(loaded_v1.session_id.is_none());
        assert!(loaded_v1.worktree_path.is_none());
        assert!(loaded_v1.branch.is_none());

        let v2 = BoardEntryV2 {
            entry_id: v1.entry_id,
            ticket_id: v1.ticket_id,
            agent_id: v1.agent_id,
            previous_attempt: v1.previous_attempt,
            checked_in_at: v1.checked_in_at,
            last_heartbeat: v1.last_heartbeat,
            ttl_secs: v1.ttl_secs,
            intent: v1.intent,
            owned_files: v1.owned_files,
            status: v1.status,
            handoff_reason: v1.handoff_reason,
            completed_at: None,
        };
        let v2_bytes = bincode::serialize(&v2).unwrap();
        let loaded_v2 = deserialize_entry(&v2_bytes).unwrap();
        assert!(loaded_v2.session_id.is_none());
        assert!(loaded_v2.worktree_path.is_none());
        assert!(loaded_v2.branch.is_none());
    }
}
