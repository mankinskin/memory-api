use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
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
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
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
}
