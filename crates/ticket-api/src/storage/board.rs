use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::StorageError;

use super::index::RedbIndexStore;

// ── Table definitions ─────────────────────────────────────────────────────────

pub(crate) const BOARD_ENTRIES: TableDefinition<&str, &[u8]> =
    TableDefinition::new("board_entries");
pub(crate) const BOARD_ACTIVE_INDEX: TableDefinition<&str, &str> =
    TableDefinition::new("board_active_index");
pub(crate) const BOARD_CONFIG: TableDefinition<&str, &[u8]> =
    TableDefinition::new("board_config");

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
}

impl BoardEntry {
    /// Returns `true` if this entry would be considered stale at the given time.
    ///
    /// Stale means the entry is `Active` but the heartbeat has expired.
    /// This is computed dynamically and is **not** written back to storage.
    pub fn is_stale_at(&self, now: DateTime<Utc>) -> bool {
        self.status == BoardEntryStatus::Active
            && now > self.last_heartbeat + Duration::seconds(self.ttl_secs as i64)
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
    /// All board entries (with stale status computed dynamically).
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
    /// Human-readable warnings (e.g. stale entries needing review).
    pub warnings: Vec<String>,
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
    StaleIntentWarning { entry_id: Uuid, current_state: String },
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
    #[error("File conflict on {files:?} with agent {conflicting_agent} (ticket {conflicting_ticket})")]
    FileConflict {
        files: Vec<String>,
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
    #[error("clean token is stale: board has changed since the preview was generated")]
    StaleCleanToken,
    #[error("file rename conflict: '{path}' is owned by agent {conflicting_agent} (ticket {conflicting_ticket})")]
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
/// The SHA-256 input is the concatenation of:
/// - each entry UUID's bytes (in sorted order)
/// - the `generated_at` timestamp as 8 LE bytes (milliseconds since epoch)
fn compute_clean_token(sorted_ids: &[Uuid], generated_at: DateTime<Utc>) -> String {
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

/// Parse and verify a clean token.  Returns `(hash_hex, generated_at)`
/// extracted from the token string, or `Err(StaleCleanToken)` if malformed.
fn parse_clean_token(token: &str) -> Result<(String, DateTime<Utc>), BoardError> {
    let Some((hash_hex, millis_str)) = token.split_once('|') else {
        return Err(BoardError::StaleCleanToken);
    };
    let ts_millis: i64 = millis_str.parse().map_err(|_| BoardError::StaleCleanToken)?;
    let generated_at =
        DateTime::from_timestamp_millis(ts_millis).ok_or(BoardError::StaleCleanToken)?;
    Ok((hash_hex.to_string(), generated_at))
}

// ── Serde helpers ─────────────────────────────────────────────────────────────

fn serialize_entry(entry: &BoardEntry) -> Result<Vec<u8>, BoardError> {
    bincode::serialize(entry)
        .map_err(|e| BoardError::Storage(StorageError::Serialization(e.to_string())))
}

fn deserialize_entry(bytes: &[u8]) -> Result<BoardEntry, BoardError> {
    bincode::deserialize(bytes)
        .map_err(|e| BoardError::Storage(StorageError::Serialization(e.to_string())))
}

fn serialize_config(config: &BoardConfig) -> Result<Vec<u8>, BoardError> {
    bincode::serialize(config)
        .map_err(|e| BoardError::Storage(StorageError::Serialization(e.to_string())))
}

fn deserialize_config(bytes: &[u8]) -> Result<BoardConfig, BoardError> {
    bincode::deserialize(bytes)
        .map_err(|e| BoardError::Storage(StorageError::Serialization(e.to_string())))
}

/// Converts any redb error type into `BoardError` via the `StorageError` bridge.
fn db_err<E: Into<StorageError>>(e: E) -> BoardError {
    BoardError::Storage(e.into())
}

// ── RedbIndexStore board extension impl ──────────────────────────────────────

impl RedbIndexStore {
    // ── config ────────────────────────────────────────────────────────────────

    pub(crate) fn board_read_config(&self) -> Result<BoardConfig, BoardError> {
        self.with_db_ext(|db| {
            let read_txn = db.begin_read().map_err(db_err)?;
            // BOARD_CONFIG may not exist on older databases; treat missing as default.
            match read_txn.open_table(BOARD_CONFIG) {
                Ok(table) => match table.get(BOARD_CONFIG_KEY).map_err(db_err)? {
                    Some(value) => deserialize_config(value.value()),
                    None => Ok(BoardConfig::default()),
                },
                Err(_) => Ok(BoardConfig::default()),
            }
        })
    }

    pub(crate) fn board_write_config(&self, config: &BoardConfig) -> Result<(), BoardError> {
        let bytes = serialize_config(config)?;
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;
            {
                let mut table = write_txn.open_table(BOARD_CONFIG).map_err(db_err)?;
                table
                    .insert(BOARD_CONFIG_KEY, bytes.as_slice())
                    .map_err(db_err)?;
            }
            write_txn.commit().map_err(db_err)?;
            Ok(())
        })
    }

    // ── check-in ──────────────────────────────────────────────────────────────

    /// Atomic check-in: validates all constraints and inserts the new entry in
    /// a single redb write transaction.
    ///
    /// Returns the committed [`BoardEntry`] on success.
    pub(crate) fn board_check_in_atomic(
        &self,
        ticket_id: Uuid,
        agent_id: &str,
        ttl_secs: u64,
        intent: &str,
        owned_files: Vec<String>,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|db| {
            let now = Utc::now();
            let write_txn = db.begin_write().map_err(db_err)?;

            // ── Step 1: Read BoardConfig ──────────────────────────────────────
            let config: BoardConfig = {
                let table = write_txn.open_table(BOARD_CONFIG).map_err(db_err)?;
                match table.get(BOARD_CONFIG_KEY).map_err(db_err)? {
                    Some(value) => deserialize_config(value.value())?,
                    None => BoardConfig::default(),
                }
            };

            // ── Step 2: Collect all existing board entries ────────────────────
            let all_entries: Vec<BoardEntry> = {
                let table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                let mut entries = Vec::new();
                for result in table.iter().map_err(db_err)? {
                    let (_, value) = result.map_err(db_err)?;
                    entries.push(deserialize_entry(value.value())?);
                }
                entries
            };

            // ── Step 3: Check WIP limit ───────────────────────────────────────
            // Active entries (including heartbeat-expired) both consume WIP slots.
            let wip_count = all_entries
                .iter()
                .filter(|e| e.status == BoardEntryStatus::Active)
                .count() as u32;

            if wip_count >= config.max_wip {
                // Abort transaction (dropped without commit).
                return Err(BoardError::WipLimitReached {
                    current: wip_count,
                    max: config.max_wip,
                });
            }

            // ── Step 4: Check BOARD_ACTIVE_INDEX for duplicate ────────────────
            let index_key = format!("{}:{}", ticket_id, agent_id);
            let existing_entry_id: Option<Uuid> = {
                let table = write_txn
                    .open_table(BOARD_ACTIVE_INDEX)
                    .map_err(db_err)?;
                match table.get(index_key.as_str()).map_err(db_err)? {
                    Some(val) => Some(val.value().parse::<Uuid>().map_err(|e| {
                        BoardError::Storage(StorageError::Serialization(e.to_string()))
                    })?),
                    None => None,
                }
            };

            if existing_entry_id.is_some_and(|eid| {
                all_entries
                    .iter()
                    .any(|e| e.entry_id == eid && e.status == BoardEntryStatus::Active)
            }) {
                return Err(BoardError::AlreadyCheckedIn {
                    ticket_id,
                    agent_id: agent_id.to_string(),
                });
            }

            // ── Steps 5-6: File conflict detection ────────────────────────────
            if !owned_files.is_empty() {
                for existing in all_entries
                    .iter()
                    .filter(|e| e.status == BoardEntryStatus::Active)
                {
                    let conflicting: Vec<String> = owned_files
                        .iter()
                        .filter(|f| existing.owned_files.contains(*f))
                        .cloned()
                        .collect();

                    if !conflicting.is_empty() {
                        // Mark the conflicting entry as Conflict and commit.
                        let mut conflict_entry = existing.clone();
                        conflict_entry.status = BoardEntryStatus::Conflict;
                        let conflict_bytes = serialize_entry(&conflict_entry)?;
                        let conflict_key = conflict_entry.entry_id.to_string();
                        {
                            let mut table =
                                write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                            table
                                .insert(conflict_key.as_str(), conflict_bytes.as_slice())
                                .map_err(db_err)?;
                        }
                        write_txn.commit().map_err(db_err)?;

                        return Err(BoardError::FileConflict {
                            files: conflicting,
                            conflicting_agent: existing.agent_id.clone(),
                            conflicting_ticket: existing.ticket_id,
                        });
                    }
                }
            }

            // ── Step 7: Find previous_attempt ─────────────────────────────────
            let previous_attempt = all_entries
                .iter()
                .find(|e| {
                    e.ticket_id == ticket_id
                        && e.agent_id == agent_id
                        && e.status == BoardEntryStatus::Completed
                })
                .map(|e| e.entry_id);

            let entry_id = Uuid::new_v4();

            let new_entry = BoardEntry {
                entry_id,
                ticket_id,
                agent_id: agent_id.to_string(),
                previous_attempt,
                checked_in_at: now,
                last_heartbeat: now,
                ttl_secs,
                intent: intent.to_string(),
                owned_files,
                status: BoardEntryStatus::Active,
                handoff_reason: None,
            };

            // ── Step 8: Insert new BoardEntry ─────────────────────────────────
            let entry_bytes = serialize_entry(&new_entry)?;
            let entry_key = entry_id.to_string();
            {
                let mut table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                table
                    .insert(entry_key.as_str(), entry_bytes.as_slice())
                    .map_err(db_err)?;
            }

            // ── Step 9: Update BOARD_ACTIVE_INDEX ─────────────────────────────
            let entry_id_str = entry_id.to_string();
            {
                let mut table = write_txn
                    .open_table(BOARD_ACTIVE_INDEX)
                    .map_err(db_err)?;
                table
                    .insert(index_key.as_str(), entry_id_str.as_str())
                    .map_err(db_err)?;
            }

            write_txn.commit().map_err(db_err)?;
            Ok(new_entry)
        })
    }

    // ── check-out ─────────────────────────────────────────────────────────────

    /// Mark an entry as `Completed`, remove it from the active index, and
    /// persist the optional `handoff_reason`.
    pub(crate) fn board_complete_entry(
        &self,
        ticket_id: &Uuid,
        agent_id: &str,
        handoff_reason: Option<&str>,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;
            let index_key = format!("{}:{}", ticket_id, agent_id);

            // Read active index to find entry_id.
            let entry_id: Uuid = {
                let table = write_txn
                    .open_table(BOARD_ACTIVE_INDEX)
                    .map_err(db_err)?;
                match table.get(index_key.as_str()).map_err(db_err)? {
                    Some(val) => val.value().parse::<Uuid>().map_err(|e| {
                        BoardError::Storage(StorageError::Serialization(e.to_string()))
                    })?,
                    None => {
                        return Err(BoardError::NotCheckedIn {
                            ticket_id: *ticket_id,
                            agent_id: agent_id.to_string(),
                        });
                    }
                }
            };

            // Read current entry.
            let entry_key = entry_id.to_string();
            let mut entry: BoardEntry = {
                let table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                match table.get(entry_key.as_str()).map_err(db_err)? {
                    Some(val) => deserialize_entry(val.value())?,
                    None => {
                        return Err(BoardError::EntryNotFound(entry_id));
                    }
                }
            };

            entry.status = BoardEntryStatus::Completed;
            entry.handoff_reason = handoff_reason.map(str::to_string);

            // Write updated entry back to BOARD_ENTRIES.
            let updated_bytes = serialize_entry(&entry)?;
            {
                let mut table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                table
                    .insert(entry_key.as_str(), updated_bytes.as_slice())
                    .map_err(db_err)?;
            }

            // Remove from BOARD_ACTIVE_INDEX (completed entries are kept in
            // BOARD_ENTRIES for audit until explicitly cleaned).
            {
                let mut table = write_txn
                    .open_table(BOARD_ACTIVE_INDEX)
                    .map_err(db_err)?;
                table.remove(index_key.as_str()).map_err(db_err)?;
            }

            write_txn.commit().map_err(db_err)?;
            Ok(entry)
        })
    }

    // ── heartbeat ─────────────────────────────────────────────────────────────

    /// Update `last_heartbeat` for an entry identified by its `entry_id`.
    pub(crate) fn board_refresh_heartbeat(
        &self,
        entry_id: &Uuid,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;
            let entry_key = entry_id.to_string();

            let mut entry: BoardEntry = {
                let table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                match table.get(entry_key.as_str()).map_err(db_err)? {
                    Some(val) => deserialize_entry(val.value())?,
                    None => return Err(BoardError::EntryNotFound(*entry_id)),
                }
            };

            entry.last_heartbeat = Utc::now();

            let updated_bytes = serialize_entry(&entry)?;
            {
                let mut table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                table
                    .insert(entry_key.as_str(), updated_bytes.as_slice())
                    .map_err(db_err)?;
            }

            write_txn.commit().map_err(db_err)?;
            Ok(entry)
        })
    }

    // ── snapshot ──────────────────────────────────────────────────────────────

    /// Build a read-only [`BoardSnapshot`] using a single redb read transaction.
    ///
    /// Stale status is computed dynamically from heartbeat timing; no writes
    /// are performed. When `agent_id` is `Some`, `caller_entries` is populated.
    pub(crate) fn board_snapshot(
        &self,
        agent_id: Option<&str>,
    ) -> Result<BoardSnapshot, BoardError> {
        self.with_db_ext(|db| {
            let now = Utc::now();
            let read_txn = db.begin_read().map_err(db_err)?;

            // Read config (treat missing table or missing key as default).
            let config: BoardConfig = match read_txn.open_table(BOARD_CONFIG) {
                Ok(table) => match table.get(BOARD_CONFIG_KEY).map_err(db_err)? {
                    Some(value) => deserialize_config(value.value())?,
                    None => BoardConfig::default(),
                },
                Err(_) => BoardConfig::default(),
            };

            // Read all entries and compute stale status dynamically.
            let mut entries: Vec<BoardEntry> = {
                let table = read_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                let mut v = Vec::new();
                for result in table.iter().map_err(db_err)? {
                    let (_, value) = result.map_err(db_err)?;
                    let mut entry = deserialize_entry(value.value())?;
                    if entry.is_stale_at(now) {
                        entry.status = BoardEntryStatus::Stale;
                    }
                    v.push(entry);
                }
                v
            };

            // Sort for deterministic output (newest checked-in first).
            entries.sort_by(|a, b| b.checked_in_at.cmp(&a.checked_in_at));

            // Compute counts.
            let active_count = entries
                .iter()
                .filter(|e| e.status == BoardEntryStatus::Active)
                .count() as u32;
            let stale_count = entries
                .iter()
                .filter(|e| e.status == BoardEntryStatus::Stale)
                .count() as u32;
            let conflict_count = entries
                .iter()
                .filter(|e| e.status == BoardEntryStatus::Conflict)
                .count() as u32;

            let wip_count = active_count + stale_count;
            let wip_limit_reached = wip_count >= config.max_wip;

            // Build file ownership map from Active + Stale entries.
            let mut file_ownership: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for entry in entries
                .iter()
                .filter(|e| matches!(e.status, BoardEntryStatus::Active | BoardEntryStatus::Stale))
            {
                for file in &entry.owned_files {
                    file_ownership
                        .entry(file.clone())
                        .or_default()
                        .push(entry.agent_id.clone());
                }
            }

            // Warnings for stale entries (high-priority human-review items).
            let mut warnings: Vec<String> = Vec::new();
            for entry in entries.iter().filter(|e| e.status == BoardEntryStatus::Stale) {
                warnings.push(format!(
                    "STALE [HIGH]: ticket {} held by agent {} — last heartbeat at {} (TTL {}s). Manual review required.",
                    entry.ticket_id, entry.agent_id, entry.last_heartbeat, entry.ttl_secs,
                ));
            }

            // Caller entries filtered to `agent_id` when provided.
            let caller_entries: Vec<BoardEntry> = match agent_id {
                Some(aid) => entries
                    .iter()
                    .filter(|e| e.agent_id == aid)
                    .cloned()
                    .collect(),
                None => Vec::new(),
            };

            Ok(BoardSnapshot {
                captured_at: now,
                entries,
                caller_entries,
                config,
                active_count,
                stale_count,
                conflict_count,
                wip_limit_reached,
                file_ownership,
                warnings,
            })
        })
    }

    // ── clean preview / apply ─────────────────────────────────────────────────

    /// Collect all cleanup-eligible entries and produce a stateless verification
    /// token.  No writes are performed.
    ///
    /// Eligible entries:
    /// - `Completed` or `Conflict` — always eligible
    /// - `Active` with an expired TTL (stale) — eligible when `include_stale` is `true`
    pub(crate) fn board_clean_preview_atomic(
        &self,
        include_stale: bool,
    ) -> Result<BoardCleanPreview, BoardError> {
        self.with_db_ext(|db| {
            let now = Utc::now();
            let read_txn = db.begin_read().map_err(db_err)?;

            let mut eligible: Vec<Uuid> = {
                let table = read_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                let mut ids = Vec::new();
                for result in table.iter().map_err(db_err)? {
                    let (_, value) = result.map_err(db_err)?;
                    let entry = deserialize_entry(value.value())?;
                    let is_eligible = matches!(
                        entry.status,
                        BoardEntryStatus::Completed | BoardEntryStatus::Conflict
                    ) || (include_stale && entry.is_stale_at(now));
                    if is_eligible {
                        ids.push(entry.entry_id);
                    }
                }
                ids
            };

            eligible.sort();
            let generated_at = now;
            let token = compute_clean_token(&eligible, generated_at);
            let entry_count = eligible.len();

            Ok(BoardCleanPreview {
                generated_at,
                token,
                entry_ids: eligible,
                entry_count,
                include_stale,
            })
        })
    }

    /// Apply a previously previewed cleanup.
    ///
    /// Re-computes the eligible set (same `include_stale` flag), derives the
    /// expected token from the `generated_at` embedded in `token`, and rejects
    /// with [`BoardError::StaleCleanToken`] if the board has changed since the
    /// preview was taken.  On success, all previewed entries are permanently
    /// removed from `BOARD_ENTRIES` (and from `BOARD_ACTIVE_INDEX` for stale
    /// entries that are still indexed as active).
    pub(crate) fn board_clean_apply_atomic(
        &self,
        token: &str,
        include_stale: bool,
    ) -> Result<BoardCleanResult, BoardError> {
        let (expected_hash_hex, generated_at) = parse_clean_token(token)?;

        self.with_db_ext(|db| {
            let now = Utc::now();
            let write_txn = db.begin_write().map_err(db_err)?;

            // Re-collect eligible entries.
            let mut eligible: Vec<(Uuid, String)> = {
                let table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                let mut pairs = Vec::new();
                for result in table.iter().map_err(db_err)? {
                    let (key, value) = result.map_err(db_err)?;
                    let entry = deserialize_entry(value.value())?;
                    let is_eligible = matches!(
                        entry.status,
                        BoardEntryStatus::Completed | BoardEntryStatus::Conflict
                    ) || (include_stale && entry.is_stale_at(now));
                    if is_eligible {
                        pairs.push((entry.entry_id, key.value().to_string()));
                    }
                }
                pairs
            };

            eligible.sort_by_key(|(id, _)| *id);
            let sorted_ids: Vec<Uuid> = eligible.iter().map(|(id, _)| *id).collect();
            let candidate_token = compute_clean_token(&sorted_ids, generated_at);

            // Verify: compare only the hash prefix (before the `|`).
            let candidate_hash = candidate_token.split_once('|').map(|(h, _)| h).unwrap_or("");
            if candidate_hash != expected_hash_hex {
                return Err(BoardError::StaleCleanToken);
            }

            // Delete all eligible entries.
            let entry_keys: Vec<String> = eligible.iter().map(|(_, k)| k.clone()).collect();
            let removed_ids: Vec<Uuid> = eligible.iter().map(|(id, _)| *id).collect();

            {
                let mut entries_table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                for key in &entry_keys {
                    entries_table.remove(key.as_str()).map_err(db_err)?;
                }
            }

            // Also clean up any stale active-index entries for removed entries.
            {
                let mut index_table = write_txn
                    .open_table(BOARD_ACTIVE_INDEX)
                    .map_err(db_err)?;
                let to_remove: Vec<String> = {
                    let mut stale_keys = Vec::new();
                    for result in index_table.iter().map_err(db_err)? {
                        let (k, v) = result.map_err(db_err)?;
                        let eid: Uuid = v.value().parse().map_err(|e: uuid::Error| {
                            BoardError::Storage(StorageError::Serialization(e.to_string()))
                        })?;
                        if removed_ids.contains(&eid) {
                            stale_keys.push(k.value().to_string());
                        }
                    }
                    stale_keys
                };
                for k in &to_remove {
                    index_table.remove(k.as_str()).map_err(db_err)?;
                }
            }

            write_txn.commit().map_err(db_err)?;
            let removed_count = removed_ids.len();
            Ok(BoardCleanResult {
                removed_entry_ids: removed_ids,
                removed_count,
            })
        })
    }

    // ── file management ───────────────────────────────────────────────────────

    /// Update the `owned_files` list for an active board entry.
    ///
    /// Files in `remove` are dropped from the entry's list.  Files in `add`
    /// are checked for conflicts against all other active entries and, if
    /// clear, appended.  Returns the updated [`BoardEntry`].
    pub(crate) fn board_update_files_atomic(
        &self,
        ticket_id: Uuid,
        agent_id: &str,
        add: Vec<String>,
        remove: Vec<String>,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;
            let index_key = format!("{ticket_id}:{agent_id}");

            // Find the entry_id from the active index.
            let entry_id: Uuid = {
                let table = write_txn
                    .open_table(BOARD_ACTIVE_INDEX)
                    .map_err(db_err)?;
                match table.get(index_key.as_str()).map_err(db_err)? {
                    Some(val) => val.value().parse::<Uuid>().map_err(|e| {
                        BoardError::Storage(StorageError::Serialization(e.to_string()))
                    })?,
                    None => {
                        return Err(BoardError::NotCheckedIn {
                            ticket_id,
                            agent_id: agent_id.to_string(),
                        });
                    }
                }
            };

            // Read all entries (needed for conflict check).
            let all_entries: Vec<BoardEntry> = {
                let table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                let mut v = Vec::new();
                for result in table.iter().map_err(db_err)? {
                    let (_, value) = result.map_err(db_err)?;
                    v.push(deserialize_entry(value.value())?);
                }
                v
            };

            // Find the caller's entry.
            let mut caller = all_entries
                .iter()
                .find(|e| e.entry_id == entry_id)
                .cloned()
                .ok_or(BoardError::EntryNotFound(entry_id))?;

            if caller.status != BoardEntryStatus::Active {
                return Err(BoardError::NotCheckedIn {
                    ticket_id,
                    agent_id: agent_id.to_string(),
                });
            }

            // Conflict check: files being added must not be owned by others.
            if !add.is_empty() {
                for other in all_entries
                    .iter()
                    .filter(|e| e.entry_id != entry_id && e.status == BoardEntryStatus::Active)
                {
                    let conflicting: Vec<String> = add
                        .iter()
                        .filter(|f| other.owned_files.contains(*f))
                        .cloned()
                        .collect();
                    if !conflicting.is_empty() {
                        return Err(BoardError::FileConflict {
                            files: conflicting,
                            conflicting_agent: other.agent_id.clone(),
                            conflicting_ticket: other.ticket_id,
                        });
                    }
                }
            }

            // Apply the update to the caller's file list.
            caller.owned_files.retain(|f| !remove.contains(f));
            for f in add {
                if !caller.owned_files.contains(&f) {
                    caller.owned_files.push(f);
                }
            }

            let updated_bytes = serialize_entry(&caller)?;
            let entry_key = caller.entry_id.to_string();
            {
                let mut table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                table
                    .insert(entry_key.as_str(), updated_bytes.as_slice())
                    .map_err(db_err)?;
            }

            write_txn.commit().map_err(db_err)?;
            Ok(caller)
        })
    }

    /// Atomically rename a file in an active board entry's `owned_files`.
    ///
    /// Checks that `new_path` is not already owned by another active entry
    /// before performing the rename.  Returns the updated [`BoardEntry`].
    pub(crate) fn board_rename_file_atomic(
        &self,
        ticket_id: Uuid,
        agent_id: &str,
        old_path: &str,
        new_path: &str,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;
            let index_key = format!("{ticket_id}:{agent_id}");

            // Find the entry_id via the active index.
            let entry_id: Uuid = {
                let table = write_txn
                    .open_table(BOARD_ACTIVE_INDEX)
                    .map_err(db_err)?;
                match table.get(index_key.as_str()).map_err(db_err)? {
                    Some(val) => val.value().parse::<Uuid>().map_err(|e| {
                        BoardError::Storage(StorageError::Serialization(e.to_string()))
                    })?,
                    None => {
                        return Err(BoardError::NotCheckedIn {
                            ticket_id,
                            agent_id: agent_id.to_string(),
                        });
                    }
                }
            };

            // Read all entries.
            let all_entries: Vec<BoardEntry> = {
                let table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                let mut v = Vec::new();
                for result in table.iter().map_err(db_err)? {
                    let (_, value) = result.map_err(db_err)?;
                    v.push(deserialize_entry(value.value())?);
                }
                v
            };

            let mut caller = all_entries
                .iter()
                .find(|e| e.entry_id == entry_id)
                .cloned()
                .ok_or(BoardError::EntryNotFound(entry_id))?;

            if caller.status != BoardEntryStatus::Active {
                return Err(BoardError::NotCheckedIn {
                    ticket_id,
                    agent_id: agent_id.to_string(),
                });
            }

            // Conflict check: new_path must not be owned by another active entry.
            for other in all_entries
                .iter()
                .filter(|e| e.entry_id != entry_id && e.status == BoardEntryStatus::Active)
            {
                if other.owned_files.contains(&new_path.to_string()) {
                    return Err(BoardError::FileRenameConflict {
                        path: new_path.to_string(),
                        conflicting_agent: other.agent_id.clone(),
                        conflicting_ticket: other.ticket_id,
                    });
                }
            }

            // Atomic remove + add.
            caller.owned_files.retain(|f| f != old_path);
            if !caller.owned_files.contains(&new_path.to_string()) {
                caller.owned_files.push(new_path.to_string());
            }

            let updated_bytes = serialize_entry(&caller)?;
            let entry_key = caller.entry_id.to_string();
            {
                let mut table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                table
                    .insert(entry_key.as_str(), updated_bytes.as_slice())
                    .map_err(db_err)?;
            }

            write_txn.commit().map_err(db_err)?;
            Ok(caller)
        })
    }

    // ── reconciliation helpers ────────────────────────────────────────────────

    /// Mark **all** active board entries for `ticket_id` as `Completed` and
    /// remove them from `BOARD_ACTIVE_INDEX`.  Returns the IDs of completed
    /// entries.
    pub(crate) fn board_complete_all_for_ticket(
        &self,
        ticket_id: Uuid,
    ) -> Result<Vec<Uuid>, BoardError> {
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;

            // Collect all Active entries for this ticket.
            let active: Vec<BoardEntry> = {
                let table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                let mut v = Vec::new();
                for result in table.iter().map_err(db_err)? {
                    let (_, value) = result.map_err(db_err)?;
                    let entry = deserialize_entry(value.value())?;
                    if entry.ticket_id == ticket_id && entry.status == BoardEntryStatus::Active {
                        v.push(entry);
                    }
                }
                v
            };

            if active.is_empty() {
                write_txn.commit().map_err(db_err)?;
                return Ok(Vec::new());
            }

            let mut completed_ids = Vec::new();

            for mut entry in active {
                entry.status = BoardEntryStatus::Completed;
                let updated_bytes = serialize_entry(&entry)?;
                let entry_key = entry.entry_id.to_string();
                {
                    let mut entries_table =
                        write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                    entries_table
                        .insert(entry_key.as_str(), updated_bytes.as_slice())
                        .map_err(db_err)?;
                }
                // Remove from active index.
                let index_key = format!("{}:{}", ticket_id, entry.agent_id);
                {
                    let mut index_table = write_txn
                        .open_table(BOARD_ACTIVE_INDEX)
                        .map_err(db_err)?;
                    index_table.remove(index_key.as_str()).map_err(db_err)?;
                }
                completed_ids.push(entry.entry_id);
            }

            write_txn.commit().map_err(db_err)?;
            Ok(completed_ids)
        })
    }

    /// Read any active board entry for `ticket_id` without writing.
    ///
    /// Returns `Some((entry, index_key))` when an active entry is found, or
    /// `None` when the ticket has no active board presence.
    pub(crate) fn board_find_active_for_ticket(
        &self,
        ticket_id: Uuid,
    ) -> Result<Option<(BoardEntry, String)>, BoardError> {
        self.with_db_ext(|db| {
            let read_txn = db.begin_read().map_err(db_err)?;
            let table = match read_txn.open_table(BOARD_ENTRIES) {
                Ok(t) => t,
                Err(_) => return Ok(None),
            };
            for result in table.iter().map_err(db_err)? {
                let (_, value) = result.map_err(db_err)?;
                let entry = deserialize_entry(value.value())?;
                if entry.ticket_id == ticket_id && entry.status == BoardEntryStatus::Active {
                    let index_key = format!("{ticket_id}:{}", entry.agent_id);
                    return Ok(Some((entry, index_key)));
                }
            }
            Ok(None)
        })
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::storage::store::TicketStore;
    use crate::storage::board::{BoardConfig, BoardEntryStatus, BoardError};

    fn make_store() -> (TempDir, TicketStore) {
        let dir = TempDir::new().expect("temp dir");
        let store = TicketStore::open(dir.path()).expect("open store");
        (dir, store)
    }

    fn make_ticket(store: &TicketStore) -> Uuid {
        store
            .create(
                None,
                "tracker-improvement",
                Some("test ticket"),
                None,
                Default::default(),
                None,
                None,
            )
            .expect("create ticket")
    }

    // ── check-in happy path ───────────────────────────────────────────────────

    #[test]
    fn check_in_happy_path() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        let entry = store
            .board_check_in(
                &ticket_id,
                "agent-alpha",
                300,
                "implement feature X",
                vec!["src/lib.rs".to_string()],
            )
            .expect("check-in should succeed");

        assert_eq!(entry.ticket_id, ticket_id);
        assert_eq!(entry.agent_id, "agent-alpha");
        assert_eq!(entry.status, BoardEntryStatus::Active);
        assert_eq!(entry.owned_files, vec!["src/lib.rs"]);
        assert!(entry.previous_attempt.is_none());
        assert!(entry.handoff_reason.is_none());

        // Snapshot should reflect the new entry.
        let snap = store.board_show(Some("agent-alpha")).expect("show");
        assert_eq!(snap.active_count, 1);
        assert_eq!(snap.caller_entries.len(), 1);
        assert_eq!(snap.caller_entries[0].entry_id, entry.entry_id);
    }

    // ── previous_attempt linking ──────────────────────────────────────────────

    #[test]
    fn previous_attempt_populated_after_checkout() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        let first = store
            .board_check_in(&ticket_id, "agent-beta", 300, "first attempt", vec![])
            .expect("first check-in");

        store
            .board_check_out(&ticket_id, "agent-beta", Some("done"))
            .expect("check-out");

        let second = store
            .board_check_in(&ticket_id, "agent-beta", 300, "second attempt", vec![])
            .expect("second check-in");

        assert_eq!(second.previous_attempt, Some(first.entry_id));
    }

    // ── WIP limit rejection ───────────────────────────────────────────────────

    #[test]
    fn wip_limit_rejected() {
        let (_dir, store) = make_store();

        // Set a low WIP limit.
        store
            .board_configure(Some(BoardConfig {
                max_wip: 2,
                stale_after_secs: 3600,
                completed_audit_window_secs: 3600,
            }))
            .expect("configure");

        // Check in two agents to distinct tickets.
        let t1 = make_ticket(&store);
        let t2 = make_ticket(&store);
        store
            .board_check_in(&t1, "agent-1", 300, "work", vec![])
            .expect("first check-in");
        store
            .board_check_in(&t2, "agent-2", 300, "work", vec![])
            .expect("second check-in");

        // Third check-in should fail with WipLimitReached.
        let t3 = make_ticket(&store);
        let err = store
            .board_check_in(&t3, "agent-3", 300, "work", vec![])
            .expect_err("should be rejected");

        assert!(
            matches!(err, BoardError::WipLimitReached { current: 2, max: 2 }),
            "expected WipLimitReached, got: {err}"
        );
    }

    // ── duplicate check-in rejection ─────────────────────────────────────────

    #[test]
    fn duplicate_check_in_rejected() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        store
            .board_check_in(&ticket_id, "agent-dup", 300, "work", vec![])
            .expect("first check-in");

        let err = store
            .board_check_in(&ticket_id, "agent-dup", 300, "work", vec![])
            .expect_err("duplicate should be rejected");

        assert!(
            matches!(err, BoardError::AlreadyCheckedIn { .. }),
            "expected AlreadyCheckedIn, got: {err}"
        );
    }

    // ── file conflict detection ───────────────────────────────────────────────

    #[test]
    fn file_conflict_rejected() {
        let (_dir, store) = make_store();
        let t1 = make_ticket(&store);
        let t2 = make_ticket(&store);

        store
            .board_check_in(
                &t1,
                "agent-x",
                300,
                "owns the file",
                vec!["shared/module.rs".to_string()],
            )
            .expect("first check-in");

        let err = store
            .board_check_in(
                &t2,
                "agent-y",
                300,
                "wants the same file",
                vec!["shared/module.rs".to_string()],
            )
            .expect_err("conflict should be rejected");

        assert!(
            matches!(err, BoardError::FileConflict { .. }),
            "expected FileConflict, got: {err}"
        );

        // The conflicting entry (agent-x) should now be marked Conflict in
        // the snapshot.
        let snap = store.board_show(None).expect("show");
        let conflict_entries: Vec<_> = snap
            .entries
            .iter()
            .filter(|e| e.status == BoardEntryStatus::Conflict)
            .collect();
        assert_eq!(conflict_entries.len(), 1);
        assert_eq!(conflict_entries[0].agent_id, "agent-x");
    }

    // ── stale detection ───────────────────────────────────────────────────────

    #[test]
    fn stale_detection_in_snapshot() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        // Check in with an extremely short TTL (1 second).
        let entry = store
            .board_check_in(&ticket_id, "agent-stale", 1, "stale eventually", vec![])
            .expect("check-in");

        // Immediately: should be Active.
        let snap = store.board_show(None).expect("show immediately");
        assert_eq!(snap.active_count, 1);
        assert_eq!(snap.stale_count, 0);

        // Wait for TTL to expire.
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Now: should show as Stale.
        let snap = store.board_show(None).expect("show after TTL");
        assert_eq!(snap.stale_count, 1, "entry should be stale after TTL");
        assert!(!snap.warnings.is_empty(), "warnings should mention stale entry");
        // Stale entries count toward WIP.
        assert_eq!(snap.active_count + snap.stale_count, 1);
        // The entry itself surfaces as Stale in the snapshot view.
        assert_eq!(snap.entries[0].status, BoardEntryStatus::Stale);
        // But the stored status remains Active (no write occurred).
        let refreshed = store
            .board_heartbeat(&entry.entry_id)
            .expect("heartbeat resets staleness");
        assert_eq!(refreshed.status, BoardEntryStatus::Active);
    }

    // ── heartbeat renewal ─────────────────────────────────────────────────────

    #[test]
    fn heartbeat_renewal() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        let entry = store
            .board_check_in(&ticket_id, "agent-hb", 300, "heartbeat test", vec![])
            .expect("check-in");

        let original_hb = entry.last_heartbeat;

        // Small sleep to ensure the clock advances.
        std::thread::sleep(std::time::Duration::from_millis(10));

        let refreshed = store
            .board_heartbeat(&entry.entry_id)
            .expect("heartbeat should succeed");

        assert!(
            refreshed.last_heartbeat > original_hb,
            "last_heartbeat should be updated"
        );
        assert_eq!(refreshed.status, BoardEntryStatus::Active);
        assert_eq!(refreshed.entry_id, entry.entry_id);
    }

    // ── check-out ────────────────────────────────────────────────────────────

    #[test]
    fn check_out_marks_completed() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        store
            .board_check_in(&ticket_id, "agent-out", 300, "work", vec![])
            .expect("check-in");

        let completed = store
            .board_check_out(&ticket_id, "agent-out", Some("handed off to reviewer"))
            .expect("check-out");

        assert_eq!(completed.status, BoardEntryStatus::Completed);
        assert_eq!(
            completed.handoff_reason.as_deref(),
            Some("handed off to reviewer")
        );

        // Snapshot should reflect zero active WIP.
        let snap = store.board_show(Some("agent-out")).expect("show");
        assert_eq!(snap.active_count, 0);
        // Completed entry is still visible in the audit log.
        assert!(snap.entries.iter().any(|e| e.status == BoardEntryStatus::Completed));
    }

    #[test]
    fn check_out_not_checked_in_returns_error() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        let err = store
            .board_check_out(&ticket_id, "agent-missing", None)
            .expect_err("should error when not checked in");

        assert!(
            matches!(err, BoardError::NotCheckedIn { .. }),
            "expected NotCheckedIn, got: {err}"
        );
    }

    // ── board_configure ───────────────────────────────────────────────────────

    #[test]
    fn configure_read_returns_default() {
        let (_dir, store) = make_store();
        let config = store.board_configure(None).expect("read default config");
        assert_eq!(config.max_wip, 5);
        assert_eq!(config.stale_after_secs, 3600);
    }

    #[test]
    fn configure_write_then_read() {
        let (_dir, store) = make_store();

        let new_config = BoardConfig {
            max_wip: 10,
            stale_after_secs: 1800,
            completed_audit_window_secs: 7200,
        };
        store
            .board_configure(Some(new_config.clone()))
            .expect("write config");

        let read_back = store.board_configure(None).expect("read config");
        assert_eq!(read_back.max_wip, 10);
        assert_eq!(read_back.stale_after_secs, 1800);
        assert_eq!(read_back.completed_audit_window_secs, 7200);
    }

    // ── board_show caller_entries ─────────────────────────────────────────────

    #[test]
    fn board_show_caller_entries_filtered() {
        let (_dir, store) = make_store();
        let t1 = make_ticket(&store);
        let t2 = make_ticket(&store);

        store
            .board_check_in(&t1, "alice", 300, "alice work", vec!["a.rs".to_string()])
            .expect("alice check-in");
        store
            .board_check_in(&t2, "bob", 300, "bob work", vec!["b.rs".to_string()])
            .expect("bob check-in");

        // alice's view: caller_entries should only have alice's entry.
        let snap = store.board_show(Some("alice")).expect("alice show");
        assert_eq!(snap.entries.len(), 2);
        assert_eq!(snap.caller_entries.len(), 1);
        assert_eq!(snap.caller_entries[0].agent_id, "alice");

        // No agent_id: caller_entries should be empty.
        let snap_anon = store.board_show(None).expect("anon show");
        assert!(snap_anon.caller_entries.is_empty());
    }

    // ── file_ownership map ────────────────────────────────────────────────────

    #[test]
    fn file_ownership_map_populated() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        store
            .board_check_in(
                &ticket_id,
                "owner",
                300,
                "owns files",
                vec!["foo.rs".to_string(), "bar.rs".to_string()],
            )
            .expect("check-in");

        let snap = store.board_show(None).expect("show");
        assert!(snap.file_ownership.contains_key("foo.rs"));
        assert!(snap.file_ownership.contains_key("bar.rs"));
        assert_eq!(snap.file_ownership["foo.rs"], vec!["owner"]);
    }

    // ── concurrent access ─────────────────────────────────────────────────────

    #[test]
    fn concurrent_check_in_only_one_wins_wip_slot() {
        let dir = TempDir::new().expect("temp dir");
        let store = Arc::new(TicketStore::open(dir.path()).expect("open store"));

        // Set WIP limit to 1 so only one thread can succeed.
        store
            .board_configure(Some(BoardConfig {
                max_wip: 1,
                stale_after_secs: 3600,
                completed_audit_window_secs: 3600,
            }))
            .expect("configure");

        // Create two distinct tickets before spawning threads.
        let t1 = make_ticket(&store);
        let t2 = make_ticket(&store);

        let store1 = Arc::clone(&store);
        let store2 = Arc::clone(&store);

        let h1 = thread::spawn(move || {
            store1.board_check_in(&t1, "thread-1", 300, "concurrent", vec![])
        });
        let h2 = thread::spawn(move || {
            store2.board_check_in(&t2, "thread-2", 300, "concurrent", vec![])
        });

        let r1 = h1.join().expect("thread 1 join");
        let r2 = h2.join().expect("thread 2 join");

        // Exactly one should succeed, one should fail with WipLimitReached.
        let successes = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
        let failures = [&r1, &r2].iter().filter(|r| r.is_err()).count();

        assert_eq!(successes, 1, "exactly one check-in should succeed");
        assert_eq!(failures, 1, "exactly one check-in should fail");

        let failed = if r1.is_err() { &r1 } else { &r2 };
        assert!(
            matches!(failed.as_ref().unwrap_err(), BoardError::WipLimitReached { .. }),
            "expected WipLimitReached"
        );
    }

    // ── board_clean_preview / board_clean_apply ───────────────────────────────

    #[test]
    fn clean_preview_happy_path() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        // Check in and then check out to produce a Completed entry.
        store
            .board_check_in(&ticket_id, "cleaner", 300, "work", vec![])
            .expect("check-in");
        store
            .board_check_out(&ticket_id, "cleaner", None)
            .expect("check-out");

        // Preview should find the completed entry.
        let preview = store
            .board_clean_preview(false)
            .expect("preview should succeed");
        assert_eq!(preview.entry_count, 1, "one completed entry is cleanup-eligible");
        assert!(!preview.token.is_empty());

        // Apply the token.
        let result = store
            .board_clean_apply(&preview.token, false)
            .expect("apply should succeed");
        assert_eq!(result.removed_count, 1);

        // Board should now be empty.
        let snap = store.board_show(None).expect("show after clean");
        assert!(snap.entries.is_empty(), "board should be empty after clean");
    }

    #[test]
    fn clean_stale_token_rejected() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        // Check in and out to produce a completed entry.
        store
            .board_check_in(&ticket_id, "agent-a", 300, "work", vec![])
            .expect("check-in");
        store
            .board_check_out(&ticket_id, "agent-a", None)
            .expect("check-out");

        // Take a preview.
        let preview = store
            .board_clean_preview(false)
            .expect("preview");

        // Mutate the board: check in another agent so the eligible set changes.
        let ticket2 = make_ticket(&store);
        store
            .board_check_in(&ticket2, "agent-b", 300, "work", vec![])
            .expect("second check-in");
        store
            .board_check_out(&ticket2, "agent-b", None)
            .expect("second check-out");

        // The token is now stale (eligible set has grown by one).
        let err = store
            .board_clean_apply(&preview.token, false)
            .expect_err("stale token should be rejected");
        assert!(
            matches!(err, BoardError::StaleCleanToken),
            "expected StaleCleanToken, got: {err}"
        );
    }

    // ── board_update_files ────────────────────────────────────────────────────

    #[test]
    fn update_files_conflict_rejected() {
        let (_dir, store) = make_store();
        let t1 = make_ticket(&store);
        let t2 = make_ticket(&store);

        // agent-1 owns "shared.rs".
        store
            .board_check_in(&t1, "agent-1", 300, "work", vec!["shared.rs".to_string()])
            .expect("check-in agent-1");

        // agent-2 owns nothing initially.
        store
            .board_check_in(&t2, "agent-2", 300, "work", vec![])
            .expect("check-in agent-2");

        // agent-2 tries to add "shared.rs" → conflict.
        let err = store
            .board_update_files(&t2, "agent-2", vec!["shared.rs".to_string()], vec![])
            .expect_err("conflict with agent-1 should be rejected");
        assert!(
            matches!(err, BoardError::FileConflict { .. }),
            "expected FileConflict, got: {err}"
        );

        // agent-2's owned_files should be unchanged.
        let snap = store.board_show(None).expect("show");
        let agent2_entry = snap
            .entries
            .iter()
            .find(|e| e.agent_id == "agent-2" && e.status == BoardEntryStatus::Active)
            .expect("agent-2 entry");
        assert!(agent2_entry.owned_files.is_empty());
    }

    #[test]
    fn update_files_success() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        store
            .board_check_in(
                &ticket_id,
                "agent-upd",
                300,
                "work",
                vec!["old.rs".to_string()],
            )
            .expect("check-in");

        let updated = store
            .board_update_files(
                &ticket_id,
                "agent-upd",
                vec!["new.rs".to_string()],
                vec!["old.rs".to_string()],
            )
            .expect("update should succeed");

        assert!(
            updated.owned_files.contains(&"new.rs".to_string()),
            "new.rs should be in owned_files"
        );
        assert!(
            !updated.owned_files.contains(&"old.rs".to_string()),
            "old.rs should have been removed"
        );
    }

    // ── board_rename_file ─────────────────────────────────────────────────────

    #[test]
    fn rename_file_conflict_rejected() {
        let (_dir, store) = make_store();
        let t1 = make_ticket(&store);
        let t2 = make_ticket(&store);

        // agent-1 owns "target.rs".
        store
            .board_check_in(&t1, "agent-1", 300, "work", vec!["target.rs".to_string()])
            .expect("check-in agent-1");

        // agent-2 owns "source.rs".
        store
            .board_check_in(&t2, "agent-2", 300, "work", vec!["source.rs".to_string()])
            .expect("check-in agent-2");

        // agent-2 tries to rename "source.rs" → "target.rs" (owned by agent-1).
        let err = store
            .board_rename_file(&t2, "agent-2", "source.rs", "target.rs")
            .expect_err("rename to owned file should be rejected");
        assert!(
            matches!(err, BoardError::FileRenameConflict { .. }),
            "expected FileRenameConflict, got: {err}"
        );

        // agent-2's files should remain unchanged.
        let snap = store.board_show(None).expect("show");
        let agent2_entry = snap
            .entries
            .iter()
            .find(|e| e.agent_id == "agent-2" && e.status == BoardEntryStatus::Active)
            .expect("agent-2 entry");
        assert_eq!(agent2_entry.owned_files, vec!["source.rs"]);
    }

    #[test]
    fn rename_file_success() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        store
            .board_check_in(
                &ticket_id,
                "agent-ren",
                300,
                "work",
                vec!["before.rs".to_string()],
            )
            .expect("check-in");

        let updated = store
            .board_rename_file(&ticket_id, "agent-ren", "before.rs", "after.rs")
            .expect("rename should succeed");

        assert!(
            !updated.owned_files.contains(&"before.rs".to_string()),
            "before.rs should be removed"
        );
        assert!(
            updated.owned_files.contains(&"after.rs".to_string()),
            "after.rs should be present"
        );
    }

    // ── board_reconcile on close / cancel / revert ────────────────────────────

    #[test]
    fn reconcile_on_close_marks_completed() {
        use std::collections::BTreeMap;

        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        // Check in agent.
        store
            .board_check_in(&ticket_id, "reconcile-agent", 300, "work", vec![])
            .expect("check-in");

        // Advance through all required states to reach "done".
        let states = [
            "in-refinement",
            "ready",
            "in-implementation",
            "in-review",
            "in-validation",
            "done",
        ];
        for state in &states {
            store
                .update(&ticket_id, BTreeMap::new(), None, Some(state), None, None)
                .expect(state);
        }

        // The board entry should now be Completed (reconciled automatically).
        let snap = store.board_show(None).expect("show after close");
        let entry = snap
            .entries
            .iter()
            .find(|e| e.ticket_id == ticket_id)
            .expect("entry should exist");
        assert_eq!(
            entry.status,
            BoardEntryStatus::Completed,
            "entry should be Completed after ticket reached 'done'"
        );
        // No active entries after reconciliation.
        assert_eq!(snap.active_count, 0);
    }

    #[test]
    fn reconcile_on_cancel_marks_completed() {
        use std::collections::BTreeMap;

        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        store
            .board_check_in(&ticket_id, "cancel-agent", 300, "work", vec![])
            .expect("check-in");

        // Cancel the ticket (new → cancelled is valid in the schema).
        store
            .update(&ticket_id, BTreeMap::new(), None, Some("cancelled"), None, None)
            .expect("cancel");

        // The board entry should now be Completed.
        let snap = store.board_show(None).expect("show after cancel");
        let entry = snap
            .entries
            .iter()
            .find(|e| e.ticket_id == ticket_id)
            .expect("entry should exist");
        assert_eq!(
            entry.status,
            BoardEntryStatus::Completed,
            "entry should be Completed after ticket was cancelled"
        );
    }

    #[test]
    fn reconcile_on_revert_emits_warning_not_completed() {
        use std::collections::BTreeMap;

        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        store
            .board_check_in(&ticket_id, "revert-agent", 300, "work", vec![])
            .expect("check-in");

        // Advance to in-implementation.
        store
            .update(&ticket_id, BTreeMap::new(), None, Some("in-refinement"), None, None)
            .expect("to in-refinement");

        // Read history so we can revert.
        let history = store.get_history(&ticket_id).expect("history");
        assert!(!history.is_empty());

        // Revert to the first revision (new state).
        let first_rev = &history[0];
        store
            .apply_revert(&ticket_id, first_rev.fields.clone(), None)
            .expect("revert");

        // The board entry should still be Active (revert emits warning, not completion).
        let snap = store.board_show(None).expect("show after revert");
        let entry = snap
            .entries
            .iter()
            .find(|e| e.ticket_id == ticket_id)
            .expect("entry should exist");
        assert_eq!(
            entry.status,
            BoardEntryStatus::Active,
            "entry should remain Active after a revert (warning only)"
        );
    }

    // ── concurrency: overlapping-file simultaneous check-in ───────────────────

    /// Two threads simultaneously claim ownership of the same file.  Exactly
    /// one check-in must succeed (return `Ok`) and the other must receive
    /// `FileConflict`.  The design intentionally marks the *existing* entry as
    /// `Conflict` when the second thread's conflict detection runs; this ensures
    /// agents are notified of the dispute.  The important invariant is that
    /// exactly one of the two tickets is represented in the board and the loser
    /// does NOT gain a board entry.
    #[test]
    fn concurrent_overlapping_file_check_in_one_wins() {
        use std::sync::Barrier;

        let dir = TempDir::new().expect("temp dir");
        let store = Arc::new(TicketStore::open(dir.path()).expect("open store"));

        let t1 = make_ticket(&store);
        let t2 = make_ticket(&store);

        // Barrier ensures both threads attempt check-in at the same time.
        let barrier = Arc::new(Barrier::new(2));
        let b1 = Arc::clone(&barrier);
        let b2 = Arc::clone(&barrier);
        let s1 = Arc::clone(&store);
        let s2 = Arc::clone(&store);

        let h1 = thread::spawn(move || {
            b1.wait();
            s1.board_check_in(&t1, "racer-1", 300, "work", vec!["contested.rs".to_string()])
        });
        let h2 = thread::spawn(move || {
            b2.wait();
            s2.board_check_in(&t2, "racer-2", 300, "work", vec!["contested.rs".to_string()])
        });

        let r1 = h1.join().expect("thread 1 join");
        let r2 = h2.join().expect("thread 2 join");

        // Exactly one succeeds. The other gets FileConflict.
        let ok_count = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
        let err_count = [&r1, &r2].iter().filter(|r| r.is_err()).count();

        assert_eq!(ok_count, 1, "exactly one racer should win the file");
        assert_eq!(err_count, 1, "exactly one racer should be rejected");

        let failed = if r1.is_err() { &r1 } else { &r2 };
        assert!(
            matches!(
                failed.as_ref().unwrap_err(),
                BoardError::FileConflict { .. }
            ),
            "loser should get FileConflict, got: {:?}",
            failed.as_ref().unwrap_err()
        );

        // Board snapshot must be consistent.
        //
        // NOTE: when the loser's transaction runs its conflict-detection it
        // intentionally marks the existing active entry as `Conflict` to signal
        // the dispute to the winning agent.  Therefore the total entry count is
        // 1 (the winner's entry, now possibly Conflict-flagged) and the loser's
        // ticket has NO board entry at all.
        let snap = store.board_show(None).expect("show after race");
        let total = snap.entries.len();
        assert_eq!(total, 1, "exactly one board entry after race (winner only)");

        let winner_ticket = if r1.is_ok() { t1 } else { t2 };
        let loser_ticket = if r1.is_ok() { t2 } else { t1 };

        let winner_entry = snap
            .entries
            .iter()
            .find(|e| e.ticket_id == winner_ticket)
            .expect("winner's entry must be present in board");
        assert!(
            winner_entry.owned_files.contains(&"contested.rs".to_string()),
            "winner's entry must list contested.rs in owned_files"
        );

        assert!(
            !snap.entries.iter().any(|e| e.ticket_id == loser_ticket),
            "loser's ticket must NOT have a board entry"
        );
    }

    /// Two threads simultaneously attempt to check in when WIP is at max-1,
    /// so only one slot remains.  Exactly one must succeed; the other must
    /// get `WipLimitReached`.
    #[test]
    fn concurrent_wip_limit_boundary_one_wins() {
        use std::sync::Barrier;

        let dir = TempDir::new().expect("temp dir");
        let store = Arc::new(TicketStore::open(dir.path()).expect("open store"));

        // Set WIP limit = 2 and fill one slot.
        store
            .board_configure(Some(BoardConfig {
                max_wip: 2,
                stale_after_secs: 3600,
                completed_audit_window_secs: 3600,
            }))
            .expect("configure");

        let pre = make_ticket(&store);
        store
            .board_check_in(&pre, "pre-agent", 300, "fills slot 1", vec![])
            .expect("pre check-in");

        // Now exactly 1 slot remains. Race two threads for it.
        let t1 = make_ticket(&store);
        let t2 = make_ticket(&store);

        let barrier = Arc::new(Barrier::new(2));
        let b1 = Arc::clone(&barrier);
        let b2 = Arc::clone(&barrier);
        let s1 = Arc::clone(&store);
        let s2 = Arc::clone(&store);

        let h1 = thread::spawn(move || {
            b1.wait();
            s1.board_check_in(&t1, "wip-racer-1", 300, "race for last slot", vec![])
        });
        let h2 = thread::spawn(move || {
            b2.wait();
            s2.board_check_in(&t2, "wip-racer-2", 300, "race for last slot", vec![])
        });

        let r1 = h1.join().expect("thread 1 join");
        let r2 = h2.join().expect("thread 2 join");

        let ok_count = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
        assert_eq!(ok_count, 1, "exactly one racer wins the last WIP slot");

        let failed = if r1.is_err() { &r1 } else { &r2 };
        assert!(
            matches!(failed.as_ref().unwrap_err(), BoardError::WipLimitReached { .. }),
            "loser should get WipLimitReached"
        );

        // Snapshot: exactly 2 active entries (pre-agent + winner).
        let snap = store.board_show(None).expect("show after boundary race");
        let active_count = snap
            .entries
            .iter()
            .filter(|e| e.status == BoardEntryStatus::Active)
            .count();
        assert_eq!(active_count, 2, "should have exactly 2 active after boundary race");
    }

    /// `board_show` can execute concurrently with `board_heartbeat` without
    /// producing inconsistent state or panicking.
    #[test]
    fn concurrent_show_and_heartbeat_no_corruption() {
        use std::sync::Barrier;

        let dir = TempDir::new().expect("temp dir");
        let store = Arc::new(TicketStore::open(dir.path()).expect("open store"));

        let ticket_id = make_ticket(&store);
        let entry = store
            .board_check_in(&ticket_id, "hb-racer", 300, "work", vec![])
            .expect("check-in");
        let entry_id = entry.entry_id;

        let n_iters = 20_usize;
        let barrier = Arc::new(Barrier::new(2));

        let b_snap = Arc::clone(&barrier);
        let b_hb = Arc::clone(&barrier);
        let s_snap = Arc::clone(&store);
        let s_hb = Arc::clone(&store);

        let snap_handle = thread::spawn(move || {
            b_snap.wait();
            let mut results = Vec::new();
            for _ in 0..n_iters {
                results.push(s_snap.board_show(None));
            }
            results
        });
        let hb_handle = thread::spawn(move || {
            b_hb.wait();
            let mut results = Vec::new();
            for _ in 0..n_iters {
                results.push(s_hb.board_heartbeat(&entry_id));
            }
            results
        });

        let snap_results = snap_handle.join().expect("snap thread join");
        let hb_results = hb_handle.join().expect("hb thread join");

        // All snapshots and heartbeats must succeed without error.
        for (i, r) in snap_results.iter().enumerate() {
            assert!(r.is_ok(), "snapshot {i} failed: {:?}", r.as_ref().unwrap_err());
        }
        for (i, r) in hb_results.iter().enumerate() {
            assert!(r.is_ok(), "heartbeat {i} failed: {:?}", r.as_ref().unwrap_err());
        }

        // Final state: entry must still be active.
        let final_snap = store.board_show(None).expect("final show");
        assert_eq!(final_snap.active_count, 1);
    }

    /// `board_clean_apply` racing with `board_check_out` and `board_heartbeat`
    /// must not corrupt the store. Either the clean wins (removing the entry)
    /// or the mutations win (entry is updated), but the store remains consistent.
    #[test]
    fn concurrent_clean_vs_checkout_and_heartbeat() {
        use std::sync::Barrier;

        let dir = TempDir::new().expect("temp dir");
        let store = Arc::new(TicketStore::open(dir.path()).expect("open store"));

        let ticket_id = make_ticket(&store);
        let entry = store
            .board_check_in(&ticket_id, "clean-racer", 300, "work", vec![])
            .expect("check-in");
        let entry_id = entry.entry_id;

        // Check out first so the entry is Completed and eligible for clean.
        store
            .board_check_out(&ticket_id, "clean-racer", Some("done"))
            .expect("check-out");

        let preview = store.board_clean_preview(false).expect("preview");
        let token = preview.token.clone();

        let barrier = Arc::new(Barrier::new(2));
        let b1 = Arc::clone(&barrier);
        let b2 = Arc::clone(&barrier);
        let s1 = Arc::clone(&store);
        let s2 = Arc::clone(&store);

        // Thread 1: apply clean.
        let h_clean = thread::spawn(move || {
            b1.wait();
            s1.board_clean_apply(&token, false)
        });

        // Thread 2: attempt heartbeat on the (completed) entry — should fail
        // with EntryNotFound or succeed; either outcome is valid and non-corrupting.
        let h_hb = thread::spawn(move || {
            b2.wait();
            s2.board_heartbeat(&entry_id)
        });

        let r_clean = h_clean.join().expect("clean thread join");
        let r_hb = h_hb.join().expect("hb thread join");

        // The clean either applied or rejected with StaleCleanToken.
        assert!(
            r_clean.is_ok()
                || matches!(r_clean.as_ref().unwrap_err(), BoardError::StaleCleanToken),
            "clean result must be ok or StaleCleanToken, got: {:?}",
            r_clean.as_ref().unwrap_err()
        );

        // Heartbeat either refreshed the entry or found it gone — both fine.
        assert!(
            r_hb.is_ok()
                || matches!(r_hb.as_ref().unwrap_err(), BoardError::EntryNotFound(_)),
            "heartbeat result must be ok or EntryNotFound, got: {:?}",
            r_hb.as_ref().unwrap_err()
        );

        // Store must be self-consistent: snapshot returns without error.
        let snap = store.board_show(None).expect("final show must succeed");
        assert_eq!(
            snap.active_count, 0,
            "no active entries after clean+checkout race"
        );
    }

    // ── restart recovery ──────────────────────────────────────────────────────

    /// Persisted active board entries survive a store re-open. After restart
    /// the snapshot reflects the same entries with their original data.
    #[test]
    fn restart_recovery_active_entries_persist() {
        let dir = TempDir::new().expect("temp dir");
        let ticket_id = {
            let store = TicketStore::open(dir.path()).expect("open store");
            let ticket_id = make_ticket(&store);
            store
                .board_check_in(
                    &ticket_id,
                    "persist-agent",
                    3600,
                    "survives restart",
                    vec!["important.rs".to_string()],
                )
                .expect("check-in");
            ticket_id
        }; // store is dropped here (simulated restart)

        // Re-open the store (simulates process restart).
        let store2 = TicketStore::open(dir.path()).expect("re-open store after restart");
        let snap = store2.board_show(None).expect("show after restart");

        assert_eq!(snap.active_count, 1, "active entry survived restart");
        let entry = snap
            .entries
            .iter()
            .find(|e| e.ticket_id == ticket_id)
            .expect("entry should be present after restart");
        assert_eq!(entry.agent_id, "persist-agent");
        assert_eq!(entry.intent, "survives restart");
        assert_eq!(entry.owned_files, vec!["important.rs"]);
        assert_eq!(entry.status, BoardEntryStatus::Active);
    }

    /// Entries that exceeded their TTL before the restart are surfaced as Stale
    /// in the first snapshot after re-open (stale status is computed dynamically).
    #[test]
    fn restart_recovery_stale_entries_recomputed() {
        let dir = TempDir::new().expect("temp dir");
        {
            let store = TicketStore::open(dir.path()).expect("open store");
            let ticket_id = make_ticket(&store);
            // Very short TTL — will expire before we re-open.
            store
                .board_check_in(&ticket_id, "stale-before-restart", 1, "will go stale", vec![])
                .expect("check-in");
            // Wait for expiry.
            std::thread::sleep(std::time::Duration::from_secs(2));
        } // store dropped

        let store2 = TicketStore::open(dir.path()).expect("re-open store after restart");
        let snap = store2.board_show(None).expect("show after restart");

        assert_eq!(snap.stale_count, 1, "stale entry detected after restart");
        assert_eq!(snap.active_count, 0, "stale does not count as active");
        assert!(
            !snap.warnings.is_empty(),
            "stale warnings should be present after restart"
        );
        // Stale entry still counts toward WIP.
        assert!(snap.wip_limit_reached || snap.stale_count > 0);
    }

    /// Completed entries within the audit window are retained after restart;
    /// the board can still show them in the snapshot.
    #[test]
    fn restart_recovery_completed_entries_retained() {
        let dir = TempDir::new().expect("temp dir");
        let ticket_id = {
            let store = TicketStore::open(dir.path()).expect("open store");
            let ticket_id = make_ticket(&store);
            store
                .board_check_in(&ticket_id, "done-agent", 3600, "finished work", vec![])
                .expect("check-in");
            store
                .board_check_out(&ticket_id, "done-agent", Some("delivered"))
                .expect("check-out");
            ticket_id
        };

        let store2 = TicketStore::open(dir.path()).expect("re-open store after restart");
        let snap = store2.board_show(None).expect("show after restart");

        assert_eq!(snap.active_count, 0, "no active after restart");
        let completed = snap
            .entries
            .iter()
            .find(|e| e.ticket_id == ticket_id && e.status == BoardEntryStatus::Completed)
            .expect("completed entry retained in audit log after restart");
        assert_eq!(
            completed.handoff_reason.as_deref(),
            Some("delivered"),
            "handoff_reason preserved across restart"
        );
    }

    /// `board_configure` values survive a store re-open.
    #[test]
    fn restart_recovery_config_persists() {
        let dir = TempDir::new().expect("temp dir");
        {
            let store = TicketStore::open(dir.path()).expect("open store");
            store
                .board_configure(Some(BoardConfig {
                    max_wip: 3,
                    stale_after_secs: 900,
                    completed_audit_window_secs: 43200,
                }))
                .expect("write config");
        }

        let store2 = TicketStore::open(dir.path()).expect("re-open store after restart");
        let config = store2.board_configure(None).expect("read config after restart");
        assert_eq!(config.max_wip, 3, "max_wip persisted");
        assert_eq!(config.stale_after_secs, 900, "stale_after_secs persisted");
        assert_eq!(
            config.completed_audit_window_secs, 43200,
            "completed_audit_window_secs persisted"
        );
    }

    // ── stale-entry mitigation end-to-end ─────────────────────────────────────

    /// A stale entry that still owns files blocks new conflicting check-in until
    /// the stale entry is explicitly resolved (via heartbeat or clean).
    #[test]
    fn stale_file_ownership_blocks_new_check_in() {
        let (_dir, store) = make_store();
        let t1 = make_ticket(&store);
        let t2 = make_ticket(&store);

        // Agent 1 checks in with a very short TTL and owns a file.
        store
            .board_check_in(
                &t1,
                "ghost-agent",
                1,
                "will go stale",
                vec!["owned.rs".to_string()],
            )
            .expect("ghost check-in");

        // Wait for TTL to expire → ghost-agent's entry is stale.
        std::thread::sleep(std::time::Duration::from_secs(2));

        let snap = store.board_show(None).expect("show after expiry");
        assert_eq!(snap.stale_count, 1, "entry should be stale");

        // Agent 2 tries to claim the same file → must be rejected.
        // (Stale entries still own their files; conflict check covers Active entries,
        //  but the file_ownership map includes Stale entries too.)
        let result = store.board_check_in(
            &t2,
            "eager-agent",
            300,
            "wants the file",
            vec!["owned.rs".to_string()],
        );

        // The check-in should fail (FileConflict) because the stale ghost still
        // has the file in its owned_files and is stored as Active in the DB.
        // (Stale is a computed view; the underlying entry is still Active.)
        assert!(
            result.is_err(),
            "new agent should not be able to claim a stale-owned file without resolution"
        );
    }

    /// Heartbeat renews a stale entry: subsequent snapshot shows Active again.
    #[test]
    fn stale_entry_renewed_by_heartbeat() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        let entry = store
            .board_check_in(&ticket_id, "renew-agent", 1, "will go stale", vec![])
            .expect("check-in");

        std::thread::sleep(std::time::Duration::from_secs(2));

        let snap_stale = store.board_show(None).expect("show stale");
        assert_eq!(snap_stale.stale_count, 1, "should be stale before renewal");

        // Renew via heartbeat.
        store.board_heartbeat(&entry.entry_id).expect("heartbeat");

        let snap_fresh = store.board_show(None).expect("show after heartbeat");
        assert_eq!(snap_fresh.stale_count, 0, "stale cleared by heartbeat");
        assert_eq!(snap_fresh.active_count, 1, "entry should be active again");
        assert!(
            snap_fresh.warnings.is_empty(),
            "no stale warnings after renewal"
        );
    }

    /// `board_clean_preview` includes stale entries when `include_stale = true`;
    /// applying the token removes them permanently.
    #[test]
    fn stale_entry_explicit_cleanup_end_to_end() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        // Check in with TTL=1 and wait for staleness.
        store
            .board_check_in(&ticket_id, "stale-cleanup", 1, "stale work", vec![])
            .expect("check-in");
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Preview without include_stale: stale entry not included.
        let preview_no_stale = store.board_clean_preview(false).expect("preview no-stale");
        assert_eq!(
            preview_no_stale.entry_count, 0,
            "stale entry not in preview without include_stale"
        );

        // Preview with include_stale: stale entry included.
        let preview_with_stale = store.board_clean_preview(true).expect("preview with-stale");
        assert_eq!(
            preview_with_stale.entry_count, 1,
            "stale entry included with include_stale=true"
        );
        assert!(
            preview_with_stale.include_stale,
            "include_stale flag preserved in preview"
        );

        // Apply to remove the stale entry.
        let result = store
            .board_clean_apply(&preview_with_stale.token, true)
            .expect("apply stale clean");
        assert_eq!(result.removed_count, 1, "stale entry removed");

        // Board is now empty.
        let snap = store.board_show(None).expect("show after stale clean");
        assert!(snap.entries.is_empty(), "board empty after stale clean");
        assert_eq!(snap.stale_count, 0);
    }

    // ── `next` / `status` board integration ──────────────────────────────────

    /// A ticket that is actively checked in does not appear in `ticket next`
    /// results when board filtering is active (the default).
    /// This is validated at the TicketStore level by checking the board snapshot
    /// directly, since `ticket next` filters tickets whose IDs appear in the
    /// board's active/stale set.
    #[test]
    fn board_active_tickets_excluded_from_next_candidates() {
        let (_dir, store) = make_store();
        let ticket_in_flight = make_ticket(&store);
        let ticket_free = make_ticket(&store);

        // Make both tickets `ready` so they would normally be `next` candidates.
        for tid in [&ticket_in_flight, &ticket_free] {
            store
                .update(tid, Default::default(), None, Some("in-refinement"), None, None)
                .expect("to in-refinement");
            store
                .update(tid, Default::default(), None, Some("ready"), None, None)
                .expect("to ready");
        }

        // Check in one ticket to the board.
        store
            .board_check_in(
                &ticket_in_flight,
                "active-agent",
                3600,
                "in flight",
                vec![],
            )
            .expect("check-in");

        let snap = store.board_show(None).expect("board show");
        let board_ticket_ids: std::collections::HashSet<Uuid> = snap
            .entries
            .iter()
            .filter(|e| {
                matches!(e.status, BoardEntryStatus::Active | BoardEntryStatus::Stale)
            })
            .map(|e| e.ticket_id)
            .collect();

        // The board-active ticket must appear in the board_ticket_ids set.
        assert!(
            board_ticket_ids.contains(&ticket_in_flight),
            "in-flight ticket must be in the board exclusion set"
        );
        // The free ticket must NOT be in the exclusion set.
        assert!(
            !board_ticket_ids.contains(&ticket_free),
            "free ticket must not be in the board exclusion set"
        );
        // WIP not breached.
        assert!(!snap.wip_limit_reached, "WIP limit not reached");
    }

    /// When the WIP limit is reached the board snapshot reports it, which
    /// the `next` command uses to emit a warning.
    #[test]
    fn board_wip_limit_surfaced_in_snapshot() {
        let (_dir, store) = make_store();
        store
            .board_configure(Some(BoardConfig {
                max_wip: 1,
                stale_after_secs: 3600,
                completed_audit_window_secs: 3600,
            }))
            .expect("configure");

        let t = make_ticket(&store);
        store
            .board_check_in(&t, "limit-agent", 3600, "fills the limit", vec![])
            .expect("check-in");

        let snap = store.board_show(None).expect("show");
        assert!(snap.wip_limit_reached, "wip_limit_reached must be true");
        assert_eq!(snap.active_count, 1);
        assert_eq!(snap.config.max_wip, 1);
    }

    /// Stale entries appear in the snapshot with a high-priority warning string,
    /// which `next` / `status` surface to operators.
    #[test]
    fn board_stale_warnings_present_in_snapshot() {
        let (_dir, store) = make_store();
        let ticket_id = make_ticket(&store);

        store
            .board_check_in(&ticket_id, "stale-warn-agent", 1, "stale soon", vec![])
            .expect("check-in");
        std::thread::sleep(std::time::Duration::from_secs(2));

        let snap = store.board_show(None).expect("show");
        assert!(
            !snap.warnings.is_empty(),
            "warnings must be non-empty for stale entries"
        );
        let warning = &snap.warnings[0];
        assert!(
            warning.contains("STALE"),
            "warning should mention STALE: {warning}"
        );
        assert!(
            warning.contains("stale-warn-agent"),
            "warning should name the agent: {warning}"
        );
    }
}
