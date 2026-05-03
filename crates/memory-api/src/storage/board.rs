use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::StorageError;
use crate::storage::schema::{
    TABLE_BOARD_ACTIVE_INDEX, TABLE_BOARD_CONFIG, TABLE_BOARD_ENTRIES,
};

use super::index::RedbIndexStore;

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

fn db_err(e: rusqlite::Error) -> BoardError {
    BoardError::Storage(StorageError::Database(e.to_string()))
}

// ── RedbIndexStore board extension impl ──────────────────────────────────────

impl RedbIndexStore {
    // ── config ────────────────────────────────────────────────────────────────

    pub fn board_read_config(&self) -> Result<BoardConfig, BoardError> {
        self.with_db_ext(|conn| {
            let bytes: Option<Vec<u8>> = conn
                .query_row(
                    &format!(
                        "SELECT data FROM {TABLE_BOARD_CONFIG} WHERE key = ?1"
                    ),
                    params![BOARD_CONFIG_KEY],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;
            match bytes {
                Some(b) => deserialize_config(&b),
                None => Ok(BoardConfig::default()),
            }
        })
    }

    pub fn board_write_config(&self, config: &BoardConfig) -> Result<(), BoardError> {
        let bytes = serialize_config(config)?;
        self.with_db_ext(|conn| {
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_CONFIG} (key, data) VALUES (?1, ?2)"
                ),
                params![BOARD_CONFIG_KEY, bytes],
            )
            .map_err(db_err)?;
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
        self.with_db_ext(|conn| {
            let now = Utc::now();
            conn.execute_batch("BEGIN IMMEDIATE;").map_err(db_err)?;

            let config: BoardConfig = {
                let bytes: Option<Vec<u8>> = conn
                    .query_row(
                        &format!("SELECT data FROM {TABLE_BOARD_CONFIG} WHERE key = ?1"),
                        params![BOARD_CONFIG_KEY],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(db_err)?;
                match bytes {
                    Some(b) => deserialize_config(&b)?,
                    None => BoardConfig::default(),
                }
            };

            let all_entries: Vec<BoardEntry> = load_all_entries(conn)?;

            let wip_count = all_entries
                .iter()
                .filter(|e| e.status == BoardEntryStatus::Active)
                .count() as u32;

            if wip_count >= config.max_wip {
                conn.execute_batch("ROLLBACK;").ok();
                return Err(BoardError::WipLimitReached {
                    current: wip_count,
                    max: config.max_wip,
                });
            }

            let index_key = format!("{ticket_id}:{agent_id}");
            let existing_entry_id: Option<Uuid> = conn
                .query_row(
                    &format!(
                        "SELECT value FROM {TABLE_BOARD_ACTIVE_INDEX} WHERE key = ?1"
                    ),
                    params![index_key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(db_err)?
                .and_then(|s| s.parse::<Uuid>().ok());

            if existing_entry_id.is_some_and(|eid| {
                all_entries
                    .iter()
                    .any(|e| e.entry_id == eid && e.status == BoardEntryStatus::Active)
            }) {
                conn.execute_batch("ROLLBACK;").ok();
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
                        conn.execute(
                            &format!(
                                "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                            ),
                            params![conflict_key, conflict_bytes],
                        )
                        .map_err(db_err)?;
                        conn.execute_batch("COMMIT;").map_err(db_err)?;
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
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                ),
                params![entry_key, entry_bytes],
            )
            .map_err(db_err)?;

            let entry_id_str = entry_id.to_string();
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ACTIVE_INDEX} (key, value) VALUES (?1, ?2)"
                ),
                params![index_key, entry_id_str],
            )
            .map_err(db_err)?;

            conn.execute_batch("COMMIT;").map_err(db_err)?;
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
        self.with_db_ext(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE;").map_err(db_err)?;
            let index_key = format!("{ticket_id}:{agent_id}");

            let entry_id: Uuid = match conn
                .query_row(
                    &format!(
                        "SELECT value FROM {TABLE_BOARD_ACTIVE_INDEX} WHERE key = ?1"
                    ),
                    params![index_key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(db_err)?
            {
                Some(s) => s.parse::<Uuid>().map_err(|e| {
                    BoardError::Storage(StorageError::Serialization(e.to_string()))
                })?,
                None => {
                    conn.execute_batch("ROLLBACK;").ok();
                    return Err(BoardError::NotCheckedIn {
                        ticket_id: *ticket_id,
                        agent_id: agent_id.to_string(),
                    });
                }
            };

            let entry_key = entry_id.to_string();
            let mut entry: BoardEntry = match conn
                .query_row(
                    &format!("SELECT data FROM {TABLE_BOARD_ENTRIES} WHERE id = ?1"),
                    params![entry_key],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(db_err)?
            {
                Some(b) => deserialize_entry(&b)?,
                None => {
                    conn.execute_batch("ROLLBACK;").ok();
                    return Err(BoardError::EntryNotFound(entry_id));
                }
            };

            entry.status = BoardEntryStatus::Completed;
            entry.handoff_reason = handoff_reason.map(str::to_string);

            let updated_bytes = serialize_entry(&entry)?;
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                ),
                params![entry_key, updated_bytes],
            )
            .map_err(db_err)?;

            conn.execute(
                &format!("DELETE FROM {TABLE_BOARD_ACTIVE_INDEX} WHERE key = ?1"),
                params![index_key],
            )
            .map_err(db_err)?;

            conn.execute_batch("COMMIT;").map_err(db_err)?;
            Ok(entry)
        })
    }

    // ── heartbeat ─────────────────────────────────────────────────────────────

    pub fn board_refresh_heartbeat(
        &self,
        entry_id: &Uuid,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE;").map_err(db_err)?;
            let entry_key = entry_id.to_string();

            let mut entry: BoardEntry = match conn
                .query_row(
                    &format!("SELECT data FROM {TABLE_BOARD_ENTRIES} WHERE id = ?1"),
                    params![entry_key],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(db_err)?
            {
                Some(b) => deserialize_entry(&b)?,
                None => {
                    conn.execute_batch("ROLLBACK;").ok();
                    return Err(BoardError::EntryNotFound(*entry_id));
                }
            };

            entry.last_heartbeat = Utc::now();

            let updated_bytes = serialize_entry(&entry)?;
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                ),
                params![entry_key, updated_bytes],
            )
            .map_err(db_err)?;

            conn.execute_batch("COMMIT;").map_err(db_err)?;
            Ok(entry)
        })
    }

    // ── snapshot ──────────────────────────────────────────────────────────────

    pub fn board_snapshot(
        &self,
        agent_id: Option<&str>,
    ) -> Result<BoardSnapshot, BoardError> {
        self.with_db_ext(|conn| {
            let now = Utc::now();

            let config: BoardConfig = {
                let bytes: Option<Vec<u8>> = conn
                    .query_row(
                        &format!("SELECT data FROM {TABLE_BOARD_CONFIG} WHERE key = ?1"),
                        params![BOARD_CONFIG_KEY],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(db_err)?;
                match bytes {
                    Some(b) => deserialize_config(&b)?,
                    None => BoardConfig::default(),
                }
            };

            let mut entries: Vec<BoardEntry> = load_all_entries(conn)?
                .into_iter()
                .map(|mut e| {
                    if e.is_stale_at(now) {
                        e.status = BoardEntryStatus::Stale;
                    }
                    e
                })
                .collect();

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
        self.with_db_ext(|conn| {
            let now = Utc::now();

            let mut eligible: Vec<Uuid> = load_all_entries(conn)?
                .into_iter()
                .filter(|e| {
                    matches!(
                        e.status,
                        BoardEntryStatus::Completed | BoardEntryStatus::Conflict
                    ) || (include_stale && e.is_stale_at(now))
                })
                .map(|e| e.entry_id)
                .collect();

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

        self.with_db_ext(|conn| {
            let now = Utc::now();
            conn.execute_batch("BEGIN IMMEDIATE;").map_err(db_err)?;

            let all_entries = load_all_entries(conn)?;
            let mut eligible: Vec<Uuid> = all_entries
                .iter()
                .filter(|e| {
                    matches!(
                        e.status,
                        BoardEntryStatus::Completed | BoardEntryStatus::Conflict
                    ) || (include_stale && e.is_stale_at(now))
                })
                .map(|e| e.entry_id)
                .collect();

            eligible.sort();
            let candidate_token = compute_clean_token(&eligible, generated_at);
            let candidate_hash =
                candidate_token.split_once('|').map(|(h, _)| h).unwrap_or("");
            if candidate_hash != expected_hash_hex {
                conn.execute_batch("ROLLBACK;").ok();
                return Err(BoardError::StaleCleanToken);
            }

            for id in &eligible {
                let id_str = id.to_string();
                conn.execute(
                    &format!("DELETE FROM {TABLE_BOARD_ENTRIES} WHERE id = ?1"),
                    params![id_str],
                )
                .map_err(db_err)?;
            }

            // Remove orphaned active-index entries that pointed at removed entries.
            let mut index_stmt = conn
                .prepare(&format!(
                    "SELECT key, value FROM {TABLE_BOARD_ACTIVE_INDEX}"
                ))
                .map_err(db_err)?;
            let to_remove: Vec<String> = index_stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(db_err)?
                .filter_map(|r| r.ok())
                .filter_map(|(k, v)| {
                    let eid: Uuid = v.parse().ok()?;
                    if eligible.contains(&eid) { Some(k) } else { None }
                })
                .collect();
            drop(index_stmt);
            for k in &to_remove {
                conn.execute(
                    &format!("DELETE FROM {TABLE_BOARD_ACTIVE_INDEX} WHERE key = ?1"),
                    params![k],
                )
                .map_err(db_err)?;
            }

            conn.execute_batch("COMMIT;").map_err(db_err)?;
            let removed_count = eligible.len();
            Ok(BoardCleanResult {
                removed_entry_ids: eligible,
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
        self.with_db_ext(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE;").map_err(db_err)?;
            let index_key = format!("{ticket_id}:{agent_id}");

            let entry_id: Uuid = match conn
                .query_row(
                    &format!(
                        "SELECT value FROM {TABLE_BOARD_ACTIVE_INDEX} WHERE key = ?1"
                    ),
                    params![index_key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(db_err)?
            {
                Some(s) => s.parse::<Uuid>().map_err(|e| {
                    BoardError::Storage(StorageError::Serialization(e.to_string()))
                })?,
                None => {
                    conn.execute_batch("ROLLBACK;").ok();
                    return Err(BoardError::NotCheckedIn {
                        ticket_id,
                        agent_id: agent_id.to_string(),
                    });
                }
            };

            let all_entries = load_all_entries(conn)?;

            let mut caller = all_entries
                .iter()
                .find(|e| e.entry_id == entry_id)
                .cloned()
                .ok_or(BoardError::EntryNotFound(entry_id))?;

            if caller.status != BoardEntryStatus::Active {
                conn.execute_batch("ROLLBACK;").ok();
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
                        conn.execute_batch("ROLLBACK;").ok();
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
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                ),
                params![entry_key, updated_bytes],
            )
            .map_err(db_err)?;

            conn.execute_batch("COMMIT;").map_err(db_err)?;
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
        self.with_db_ext(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE;").map_err(db_err)?;
            let index_key = format!("{ticket_id}:{agent_id}");

            let entry_id: Uuid = match conn
                .query_row(
                    &format!(
                        "SELECT value FROM {TABLE_BOARD_ACTIVE_INDEX} WHERE key = ?1"
                    ),
                    params![index_key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(db_err)?
            {
                Some(s) => s.parse::<Uuid>().map_err(|e| {
                    BoardError::Storage(StorageError::Serialization(e.to_string()))
                })?,
                None => {
                    conn.execute_batch("ROLLBACK;").ok();
                    return Err(BoardError::NotCheckedIn {
                        ticket_id,
                        agent_id: agent_id.to_string(),
                    });
                }
            };

            let all_entries = load_all_entries(conn)?;

            let mut caller = all_entries
                .iter()
                .find(|e| e.entry_id == entry_id)
                .cloned()
                .ok_or(BoardError::EntryNotFound(entry_id))?;

            if caller.status != BoardEntryStatus::Active {
                conn.execute_batch("ROLLBACK;").ok();
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
                    conn.execute_batch("ROLLBACK;").ok();
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
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                ),
                params![entry_key, updated_bytes],
            )
            .map_err(db_err)?;

            conn.execute_batch("COMMIT;").map_err(db_err)?;
            Ok(caller)
        })
    }

    // ── reconciliation helpers ────────────────────────────────────────────────

    pub fn board_complete_all_for_ticket(
        &self,
        ticket_id: Uuid,
    ) -> Result<Vec<Uuid>, BoardError> {
        self.with_db_ext(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE;").map_err(db_err)?;

            let active: Vec<BoardEntry> = load_all_entries(conn)?
                .into_iter()
                .filter(|e| {
                    e.ticket_id == ticket_id && e.status == BoardEntryStatus::Active
                })
                .collect();

            if active.is_empty() {
                conn.execute_batch("COMMIT;").map_err(db_err)?;
                return Ok(Vec::new());
            }

            let mut completed_ids = Vec::new();

            for mut entry in active {
                entry.status = BoardEntryStatus::Completed;
                let updated_bytes = serialize_entry(&entry)?;
                let entry_key = entry.entry_id.to_string();
                conn.execute(
                    &format!(
                        "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                    ),
                    params![entry_key, updated_bytes],
                )
                .map_err(db_err)?;

                let index_key = format!("{ticket_id}:{}", entry.agent_id);
                conn.execute(
                    &format!("DELETE FROM {TABLE_BOARD_ACTIVE_INDEX} WHERE key = ?1"),
                    params![index_key],
                )
                .map_err(db_err)?;

                completed_ids.push(entry.entry_id);
            }

            conn.execute_batch("COMMIT;").map_err(db_err)?;
            Ok(completed_ids)
        })
    }

    pub fn board_find_active_for_ticket(
        &self,
        ticket_id: Uuid,
    ) -> Result<Option<(BoardEntry, String)>, BoardError> {
        self.with_db_ext(|conn| {
            for entry in load_all_entries(conn)? {
                if entry.ticket_id == ticket_id && entry.status == BoardEntryStatus::Active {
                    let index_key = format!("{ticket_id}:{}", entry.agent_id);
                    return Ok(Some((entry, index_key)));
                }
            }
            Ok(None)
        })
    }
}

// ── internal helpers ──────────────────────────────────────────────────────────

fn load_all_entries(conn: &Connection) -> Result<Vec<BoardEntry>, BoardError> {
    let mut stmt = conn
        .prepare(&format!("SELECT data FROM {TABLE_BOARD_ENTRIES}"))
        .map_err(db_err)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(db_err)?;
    let mut entries = Vec::new();
    for bytes in rows {
        entries.push(deserialize_entry(&bytes.map_err(db_err)?)?);
    }
    Ok(entries)
}
