use rusqlite::params;
use uuid::Uuid;

use crate::storage::index::RedbIndexStore;
use crate::storage::schema::{TABLE_BOARD_ACTIVE_INDEX, TABLE_BOARD_ENTRIES};

use super::{load_all_entries, lookup_active_entry_id};
use super::super::{
    BoardEntry, BoardEntryStatus, BoardError, db_err, serialize_entry,
};

impl RedbIndexStore {
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
            let entry_id = lookup_active_entry_id(conn, &index_key, ticket_id, agent_id)?;
            let all_entries = load_all_entries(conn)?;

            let mut caller = all_entries
                .iter()
                .find(|entry| entry.entry_id == entry_id)
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
                for other in all_entries.iter().filter(|entry| {
                    entry.entry_id != entry_id && entry.status == BoardEntryStatus::Active
                }) {
                    let conflicting: Vec<String> = add
                        .iter()
                        .filter(|file| other.owned_files.contains(*file))
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

            caller.owned_files.retain(|file| !remove.contains(file));
            for file in add {
                if !caller.owned_files.contains(&file) {
                    caller.owned_files.push(file);
                }
            }

            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                ),
                params![caller.entry_id.to_string(), serialize_entry(&caller)?],
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
            let entry_id = lookup_active_entry_id(conn, &index_key, ticket_id, agent_id)?;
            let all_entries = load_all_entries(conn)?;

            let mut caller = all_entries
                .iter()
                .find(|entry| entry.entry_id == entry_id)
                .cloned()
                .ok_or(BoardError::EntryNotFound(entry_id))?;
            if caller.status != BoardEntryStatus::Active {
                conn.execute_batch("ROLLBACK;").ok();
                return Err(BoardError::NotCheckedIn {
                    ticket_id,
                    agent_id: agent_id.to_string(),
                });
            }

            for other in all_entries.iter().filter(|entry| {
                entry.entry_id != entry_id && entry.status == BoardEntryStatus::Active
            }) {
                if other.owned_files.contains(&new_path.to_string()) {
                    conn.execute_batch("ROLLBACK;").ok();
                    return Err(BoardError::FileRenameConflict {
                        path: new_path.to_string(),
                        conflicting_agent: other.agent_id.clone(),
                        conflicting_ticket: other.ticket_id,
                    });
                }
            }

            caller.owned_files.retain(|file| file != old_path);
            if !caller.owned_files.contains(&new_path.to_string()) {
                caller.owned_files.push(new_path.to_string());
            }

            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                ),
                params![caller.entry_id.to_string(), serialize_entry(&caller)?],
            )
            .map_err(db_err)?;

            conn.execute_batch("COMMIT;").map_err(db_err)?;
            Ok(caller)
        })
    }

    pub fn board_complete_all_for_ticket(
        &self,
        ticket_id: Uuid,
    ) -> Result<Vec<Uuid>, BoardError> {
        self.with_db_ext(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE;").map_err(db_err)?;

            let active: Vec<BoardEntry> = load_all_entries(conn)?
                .into_iter()
                .filter(|entry| {
                    entry.ticket_id == ticket_id && entry.status == BoardEntryStatus::Active
                })
                .collect();
            if active.is_empty() {
                conn.execute_batch("COMMIT;").map_err(db_err)?;
                return Ok(Vec::new());
            }

            let mut completed_ids = Vec::new();
            for mut entry in active {
                entry.status = BoardEntryStatus::Completed;
                conn.execute(
                    &format!(
                        "INSERT OR REPLACE INTO {TABLE_BOARD_ENTRIES} (id, data) VALUES (?1, ?2)"
                    ),
                    params![entry.entry_id.to_string(), serialize_entry(&entry)?],
                )
                .map_err(db_err)?;
                conn.execute(
                    &format!("DELETE FROM {TABLE_BOARD_ACTIVE_INDEX} WHERE key = ?1"),
                    params![format!("{ticket_id}:{}", entry.agent_id)],
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