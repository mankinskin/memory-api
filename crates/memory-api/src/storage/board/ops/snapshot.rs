use std::collections::BTreeMap;

use chrono::{
    Duration,
    Utc,
};

use crate::storage::index::RedbIndexStore;

use super::{
    super::{
        BoardEntry,
        BoardEntryStatus,
        BoardError,
        BoardHistorySnapshot,
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

            let current_entries: Vec<BoardEntry> = load_all_entries(conn)?
                .into_iter()
                .map(|mut entry| {
                    if entry.is_stale_at(now) {
                        entry.status = BoardEntryStatus::Stale;
                    }
                    entry
                })
                .filter(|entry| {
                    matches!(
                        entry.status,
                        BoardEntryStatus::Active
                            | BoardEntryStatus::Stale
                            | BoardEntryStatus::Conflict
                    )
                })
                .collect();
            let mut entries = current_entries;
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

    pub fn board_history_snapshot(
        &self,
        agent_id: Option<&str>,
    ) -> Result<BoardHistorySnapshot, BoardError> {
        self.with_db_ext(|conn| {
            let now = Utc::now();
            let config = read_board_config(conn)?;
            let history_cutoff = if config.completed_audit_window_secs == 0 {
                None
            } else {
                Some(
                    now - Duration::seconds(
                        config.completed_audit_window_secs as i64,
                    ),
                )
            };

            let completed_entries: Vec<BoardEntry> = load_all_entries(conn)?
                .into_iter()
                .filter(|entry| entry.status == BoardEntryStatus::Completed)
                .collect();
            let total_completed_count = completed_entries.len() as u32;

            let mut entries: Vec<BoardEntry> = completed_entries
                .into_iter()
                .filter(|entry| {
                    history_cutoff.map_or(true, |cutoff| {
                        effective_completed_at(entry) >= cutoff
                    })
                })
                .collect();
            entries.sort_by(|left, right| {
                effective_completed_at(right).cmp(&effective_completed_at(left))
            });

            let caller_entries = match agent_id {
                Some(agent_id) => entries
                    .iter()
                    .filter(|entry| entry.agent_id == agent_id)
                    .cloned()
                    .collect(),
                None => Vec::new(),
            };

            Ok(BoardHistorySnapshot {
                captured_at: now,
                completed_count: entries.len() as u32,
                hidden_completed_count: total_completed_count
                    .saturating_sub(entries.len() as u32),
                entries,
                caller_entries,
                config,
            })
        })
    }
}

fn effective_completed_at(entry: &BoardEntry) -> chrono::DateTime<Utc> {
    entry
        .completed_at
        .unwrap_or(entry.last_heartbeat.max(entry.checked_in_at))
}
