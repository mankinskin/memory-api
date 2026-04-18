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

fn db_err<E: Into<StorageError>>(e: E) -> BoardError {
    BoardError::Storage(e.into())
}

// ── RedbIndexStore board extension impl ──────────────────────────────────────

impl RedbIndexStore {
    // ── config ────────────────────────────────────────────────────────────────

    pub fn board_read_config(&self) -> Result<BoardConfig, BoardError> {
        self.with_db_ext(|db| {
            let read_txn = db.begin_read().map_err(db_err)?;
            match read_txn.open_table(BOARD_CONFIG) {
                Ok(table) => match table.get(BOARD_CONFIG_KEY).map_err(db_err)? {
                    Some(value) => deserialize_config(value.value()),
                    None => Ok(BoardConfig::default()),
                },
                Err(_) => Ok(BoardConfig::default()),
            }
        })
    }

    pub fn board_write_config(&self, config: &BoardConfig) -> Result<(), BoardError> {
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

    pub fn board_check_in_atomic(
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

            let config: BoardConfig = {
                let table = write_txn.open_table(BOARD_CONFIG).map_err(db_err)?;
                match table.get(BOARD_CONFIG_KEY).map_err(db_err)? {
                    Some(value) => deserialize_config(value.value())?,
                    None => BoardConfig::default(),
                }
            };

            let all_entries: Vec<BoardEntry> = {
                let table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                let mut entries = Vec::new();
                for result in table.iter().map_err(db_err)? {
                    let (_, value) = result.map_err(db_err)?;
                    entries.push(deserialize_entry(value.value())?);
                }
                entries
            };

            let wip_count = all_entries
                .iter()
                .filter(|e| e.status == BoardEntryStatus::Active)
                .count() as u32;

            if wip_count >= config.max_wip {
                return Err(BoardError::WipLimitReached {
                    current: wip_count,
                    max: config.max_wip,
                });
            }

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

            let entry_bytes = serialize_entry(&new_entry)?;
            let entry_key = entry_id.to_string();
            {
                let mut table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                table
                    .insert(entry_key.as_str(), entry_bytes.as_slice())
                    .map_err(db_err)?;
            }

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

    pub fn board_complete_entry(
        &self,
        ticket_id: &Uuid,
        agent_id: &str,
        handoff_reason: Option<&str>,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;
            let index_key = format!("{}:{}", ticket_id, agent_id);

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

            let updated_bytes = serialize_entry(&entry)?;
            {
                let mut table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                table
                    .insert(entry_key.as_str(), updated_bytes.as_slice())
                    .map_err(db_err)?;
            }

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

    pub fn board_refresh_heartbeat(
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

    pub fn board_snapshot(
        &self,
        agent_id: Option<&str>,
    ) -> Result<BoardSnapshot, BoardError> {
        self.with_db_ext(|db| {
            let now = Utc::now();
            let read_txn = db.begin_read().map_err(db_err)?;

            let config: BoardConfig = match read_txn.open_table(BOARD_CONFIG) {
                Ok(table) => match table.get(BOARD_CONFIG_KEY).map_err(db_err)? {
                    Some(value) => deserialize_config(value.value())?,
                    None => BoardConfig::default(),
                },
                Err(_) => BoardConfig::default(),
            };

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

            entries.sort_by(|a, b| b.checked_in_at.cmp(&a.checked_in_at));

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

            let mut warnings: Vec<String> = Vec::new();
            for entry in entries.iter().filter(|e| e.status == BoardEntryStatus::Stale) {
                warnings.push(format!(
                    "STALE [HIGH]: ticket {} held by agent {} — last heartbeat at {} (TTL {}s). Manual review required.",
                    entry.ticket_id, entry.agent_id, entry.last_heartbeat, entry.ttl_secs,
                ));
            }

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

    pub fn board_clean_preview_atomic(
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

    pub fn board_clean_apply_atomic(
        &self,
        token: &str,
        include_stale: bool,
    ) -> Result<BoardCleanResult, BoardError> {
        let (expected_hash_hex, generated_at) = parse_clean_token(token)?;

        self.with_db_ext(|db| {
            let now = Utc::now();
            let write_txn = db.begin_write().map_err(db_err)?;

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

            let candidate_hash = candidate_token.split_once('|').map(|(h, _)| h).unwrap_or("");
            if candidate_hash != expected_hash_hex {
                return Err(BoardError::StaleCleanToken);
            }

            let entry_keys: Vec<String> = eligible.iter().map(|(_, k)| k.clone()).collect();
            let removed_ids: Vec<Uuid> = eligible.iter().map(|(id, _)| *id).collect();

            {
                let mut entries_table = write_txn.open_table(BOARD_ENTRIES).map_err(db_err)?;
                for key in &entry_keys {
                    entries_table.remove(key.as_str()).map_err(db_err)?;
                }
            }

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

    pub fn board_update_files_atomic(
        &self,
        ticket_id: Uuid,
        agent_id: &str,
        add: Vec<String>,
        remove: Vec<String>,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;
            let index_key = format!("{ticket_id}:{agent_id}");

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

    pub fn board_rename_file_atomic(
        &self,
        ticket_id: Uuid,
        agent_id: &str,
        old_path: &str,
        new_path: &str,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;
            let index_key = format!("{ticket_id}:{agent_id}");

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

    pub fn board_complete_all_for_ticket(
        &self,
        ticket_id: Uuid,
    ) -> Result<Vec<Uuid>, BoardError> {
        self.with_db_ext(|db| {
            let write_txn = db.begin_write().map_err(db_err)?;

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

    pub fn board_find_active_for_ticket(
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
