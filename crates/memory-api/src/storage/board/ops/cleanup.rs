use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use crate::storage::{
    index::RedbIndexStore,
    schema::{
        TABLE_BOARD_ACTIVE_INDEX,
        TABLE_BOARD_ENTRIES,
    },
};

use super::{
    super::{
        BoardCleanPreview,
        BoardCleanResult,
        BoardEntryStatus,
        BoardError,
        compute_clean_token,
        db_err,
        parse_clean_token,
    },
    load_all_entries,
    read_board_config,
};

impl RedbIndexStore {
    pub fn board_clean_preview_atomic(
        &self,
        include_stale: bool,
    ) -> Result<BoardCleanPreview, BoardError> {
        self.with_db_ext(|conn| {
            let now = Utc::now();
            let _config = read_board_config(conn)?;
            let mut eligible: Vec<Uuid> = load_all_entries(conn)?
                .into_iter()
                .filter(|entry| {
                    matches!(
                        entry.status,
                        BoardEntryStatus::Completed
                            | BoardEntryStatus::Conflict
                    ) || (include_stale && entry.is_stale_at(now))
                })
                .map(|entry| entry.entry_id)
                .collect();
            eligible.sort();

            let generated_at = now;
            Ok(BoardCleanPreview {
                generated_at,
                token: compute_clean_token(&eligible, generated_at),
                entry_count: eligible.len(),
                entry_ids: eligible,
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

            let mut eligible: Vec<Uuid> = load_all_entries(conn)?
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.status,
                        BoardEntryStatus::Completed
                            | BoardEntryStatus::Conflict
                    ) || (include_stale && entry.is_stale_at(now))
                })
                .map(|entry| entry.entry_id)
                .collect();
            eligible.sort();

            let candidate_token = compute_clean_token(&eligible, generated_at);
            let candidate_hash = candidate_token
                .split_once('|')
                .map(|(hash, _)| hash)
                .unwrap_or("");
            if candidate_hash != expected_hash_hex {
                conn.execute_batch("ROLLBACK;").ok();
                return Err(BoardError::StaleCleanToken);
            }

            for id in &eligible {
                conn.execute(
                    &format!("DELETE FROM {TABLE_BOARD_ENTRIES} WHERE id = ?1"),
                    params![id.to_string()],
                )
                .map_err(db_err)?;
            }

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
                .filter_map(|row| row.ok())
                .filter_map(|(key, value)| {
                    let entry_id: Uuid = value.parse().ok()?;
                    eligible.contains(&entry_id).then_some(key)
                })
                .collect();
            drop(index_stmt);

            for key in &to_remove {
                conn.execute(
                    &format!(
                        "DELETE FROM {TABLE_BOARD_ACTIVE_INDEX} WHERE key = ?1"
                    ),
                    params![key],
                )
                .map_err(db_err)?;
            }

            conn.execute_batch("COMMIT;").map_err(db_err)?;
            Ok(BoardCleanResult {
                removed_count: eligible.len(),
                removed_entry_ids: eligible,
            })
        })
    }
}
