use chrono::Utc;
use rusqlite::{
    Connection,
    OptionalExtension,
    params,
};
use uuid::Uuid;

use crate::{
    error::StorageError,
    storage::{
        index::RedbIndexStore,
        schema::{
            TABLE_BOARD_ACTIVE_INDEX,
            TABLE_BOARD_CONFIG,
            TABLE_BOARD_ENTRIES,
        },
    },
};

use super::{
    BOARD_CONFIG_KEY,
    BoardConfig,
    BoardEntry,
    BoardEntryStatus,
    BoardError,
    db_err,
    deserialize_config,
    deserialize_entry,
    serialize_config,
    serialize_entry,
};

impl RedbIndexStore {
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
                Some(bytes) => deserialize_config(&bytes),
                None => Ok(BoardConfig::default()),
            }
        })
    }

    pub fn board_write_config(
        &self,
        config: &BoardConfig,
    ) -> Result<(), BoardError> {
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

            let config = read_board_config(conn)?;
            let all_entries = load_all_entries(conn)?;
            let wip_count = all_entries
                .iter()
                .filter(|entry| entry.status == BoardEntryStatus::Active)
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
                .and_then(|value| value.parse::<Uuid>().ok());

            if existing_entry_id.is_some_and(|entry_id| {
                all_entries.iter().any(|entry| {
                    entry.entry_id == entry_id && entry.status == BoardEntryStatus::Active
                })
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
                    .filter(|entry| entry.status == BoardEntryStatus::Active)
                {
                    let conflicting: Vec<String> = owned_files
                        .iter()
                        .filter(|file| existing.owned_files.contains(*file))
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
                .find(|entry| {
                    entry.ticket_id == ticket_id
                        && entry.agent_id == agent_id
                        && entry.status == BoardEntryStatus::Completed
                })
                .map(|entry| entry.entry_id);

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
                completed_at: None,
            };

            let entry_bytes = serialize_entry(&new_entry)?;
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                ),
                params![entry_id.to_string(), entry_bytes],
            )
            .map_err(db_err)?;

            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ACTIVE_INDEX} (key, value) VALUES (?1, ?2)"
                ),
                params![index_key, entry_id.to_string()],
            )
            .map_err(db_err)?;

            conn.execute_batch("COMMIT;").map_err(db_err)?;
            Ok(new_entry)
        })
    }

    pub fn board_complete_entry(
        &self,
        ticket_id: &Uuid,
        agent_id: &str,
        handoff_reason: Option<&str>,
    ) -> Result<BoardEntry, BoardError> {
        self.with_db_ext(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE;").map_err(db_err)?;
            let index_key = format!("{ticket_id}:{agent_id}");

            let entry_id = lookup_active_entry_id(conn, &index_key, *ticket_id, agent_id)?;
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
                Some(bytes) => deserialize_entry(&bytes)?,
                None => {
                    conn.execute_batch("ROLLBACK;").ok();
                    return Err(BoardError::EntryNotFound(entry_id));
                }
            };

            entry.status = BoardEntryStatus::Completed;
            entry.handoff_reason = handoff_reason.map(str::to_string);
            entry.completed_at = Some(Utc::now());

            let updated_bytes = serialize_entry(&entry)?;
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                ),
                params![entry_id.to_string(), updated_bytes],
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
                Some(bytes) => deserialize_entry(&bytes)?,
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
                params![entry_id.to_string(), updated_bytes],
            )
            .map_err(db_err)?;

            conn.execute_batch("COMMIT;").map_err(db_err)?;
            Ok(entry)
        })
    }
}

mod cleanup;
mod files;
mod snapshot;

fn read_board_config(conn: &Connection) -> Result<BoardConfig, BoardError> {
    let bytes: Option<Vec<u8>> = conn
        .query_row(
            &format!("SELECT data FROM {TABLE_BOARD_CONFIG} WHERE key = ?1"),
            params![BOARD_CONFIG_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;
    match bytes {
        Some(bytes) => deserialize_config(&bytes),
        None => Ok(BoardConfig::default()),
    }
}

fn lookup_active_entry_id(
    conn: &Connection,
    index_key: &str,
    ticket_id: Uuid,
    agent_id: &str,
) -> Result<Uuid, BoardError> {
    match conn
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
        Some(value) => value.parse::<Uuid>().map_err(|error| {
            BoardError::Storage(StorageError::Serialization(error.to_string()))
        }),
        None => Err(BoardError::NotCheckedIn {
            ticket_id,
            agent_id: agent_id.to_string(),
        }),
    }
}

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
