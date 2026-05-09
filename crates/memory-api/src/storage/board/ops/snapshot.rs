use std::collections::BTreeMap;

use chrono::Utc;

use crate::storage::index::RedbIndexStore;

use super::{
    super::{
        BoardEntry,
        BoardEntryStatus,
        BoardError,
        BoardSnapshot,
    },
    load_all_entries,
    read_board_config,
};

impl RedbIndexStore {
    pub fn board_snapshot(
        &self,
        agent_id: Option<&str>,
    ) -> Result<BoardSnapshot, BoardError> {
        self.with_db_ext(|conn| {
            let now = Utc::now();
            let config = read_board_config(conn)?;

            let mut entries: Vec<BoardEntry> = load_all_entries(conn)?
                .into_iter()
                .map(|mut entry| {
                    if entry.is_stale_at(now) {
                        entry.status = BoardEntryStatus::Stale;
                    }
                    entry
                })
                .collect();
            entries.sort_by(|left, right| right.checked_in_at.cmp(&left.checked_in_at));

            let active_count = entries
                .iter()
                .filter(|entry| entry.status == BoardEntryStatus::Active)
                .count() as u32;
            let stale_count = entries
                .iter()
                .filter(|entry| entry.status == BoardEntryStatus::Stale)
                .count() as u32;
            let conflict_count = entries
                .iter()
                .filter(|entry| entry.status == BoardEntryStatus::Conflict)
                .count() as u32;

            let mut file_ownership: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for entry in entries.iter().filter(|entry| {
                matches!(entry.status, BoardEntryStatus::Active | BoardEntryStatus::Stale)
            }) {
                for file in &entry.owned_files {
                    file_ownership
                        .entry(file.clone())
                        .or_default()
                        .push(entry.agent_id.clone());
                }
            }

            let warnings = entries
                .iter()
                .filter(|entry| entry.status == BoardEntryStatus::Stale)
                .map(|entry| {
                    format!(
                        "STALE [HIGH]: ticket {} held by agent {} — last heartbeat at {} (TTL {}s). Manual review required.",
                        entry.ticket_id, entry.agent_id, entry.last_heartbeat, entry.ttl_secs,
                    )
                })
                .collect();

            let caller_entries = match agent_id {
                Some(agent_id) => entries
                    .iter()
                    .filter(|entry| entry.agent_id == agent_id)
                    .cloned()
                    .collect(),
                None => Vec::new(),
            };

            Ok(BoardSnapshot {
                captured_at: now,
                entries,
                caller_entries,
                config: config.clone(),
                active_count,
                stale_count,
                conflict_count,
                wip_limit_reached: active_count + stale_count >= config.max_wip,
                file_ownership,
                warnings,
            })
        })
    }
}
