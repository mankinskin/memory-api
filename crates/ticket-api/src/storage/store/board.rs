use chrono::Utc;
use uuid::Uuid;

use crate::{
    error::StorageError,
    storage::{
        board::{
            BoardCleanPreview,
            BoardCleanResult,
            BoardConfig,
            BoardEntry,
            BoardError,
            BoardSnapshot,
            ReconcileAction,
        },
        indexed::{
            IndexedTicket,
            LeaseInfo,
        },
    },
};

use super::TicketStore;

impl TicketStore {
    pub fn claim(
        &self,
        ticket_id: &Uuid,
        agent_id: &str,
        ttl_secs: u64,
        work_intent: Option<&str>,
    ) -> Result<LeaseInfo, StorageError> {
        if let Some(existing) = self.index.get_lease(ticket_id)? {
            if !existing.is_expired() {
                return Err(StorageError::LeaseConflict {
                    ticket: *ticket_id,
                    holder: existing.working_by.clone(),
                });
            }
        }

        let now = Utc::now();
        let lease = LeaseInfo {
            ticket_id: *ticket_id,
            working_by: agent_id.to_string(),
            work_intent: work_intent.map(str::to_string),
            claimed_at: now,
            lease_expires_at: now + chrono::Duration::seconds(ttl_secs as i64),
            ttl_secs,
            conflict_domain: None,
        };
        self.index.insert_lease(&lease)?;
        Ok(lease)
    }

    pub fn unclaim(
        &self,
        ticket_id: &Uuid,
    ) -> Result<(), StorageError> {
        self.index.remove_lease(ticket_id)
    }

    pub fn list_leases(&self) -> Result<Vec<LeaseInfo>, StorageError> {
        self.index.list_active_leases()
    }

    pub fn board_check_in(
        &self,
        ticket_id: &Uuid,
        agent_id: &str,
        ttl_secs: u64,
        intent: &str,
        owned_files: Vec<String>,
    ) -> Result<BoardEntry, BoardError> {
        let entry = self.index.board_check_in_atomic(
            *ticket_id,
            agent_id,
            ttl_secs,
            intent,
            owned_files,
        )?;

        match self.claim(ticket_id, agent_id, ttl_secs, Some(intent)) {
            Ok(_) | Err(StorageError::LeaseConflict { .. }) => {},
            Err(error) => return Err(BoardError::Storage(error)),
        }

        Ok(entry)
    }

    pub fn board_check_out(
        &self,
        ticket_id: &Uuid,
        agent_id: &str,
        handoff_reason: Option<&str>,
    ) -> Result<BoardEntry, BoardError> {
        let entry = self.index.board_complete_entry(
            ticket_id,
            agent_id,
            handoff_reason,
        )?;

        match self.unclaim(ticket_id) {
            Ok(_) | Err(StorageError::NotFound(_)) => {},
            Err(error) => return Err(BoardError::Storage(error)),
        }

        Ok(entry)
    }

    pub fn board_heartbeat(
        &self,
        entry_id: &Uuid,
    ) -> Result<BoardEntry, BoardError> {
        self.index.board_refresh_heartbeat(entry_id)
    }

    pub fn board_show(
        &self,
        agent_id: Option<&str>,
    ) -> Result<BoardSnapshot, BoardError> {
        self.index.board_snapshot(agent_id)
    }

    pub fn board_configure(
        &self,
        config: Option<BoardConfig>,
    ) -> Result<BoardConfig, BoardError> {
        match config {
            None => self.index.board_read_config(),
            Some(config) => {
                self.index.board_write_config(&config)?;
                Ok(config)
            },
        }
    }

    pub fn board_clean_preview(
        &self,
        include_stale: bool,
    ) -> Result<BoardCleanPreview, BoardError> {
        self.index.board_clean_preview_atomic(include_stale)
    }

    pub fn board_clean_apply(
        &self,
        token: &str,
        include_stale: bool,
    ) -> Result<BoardCleanResult, BoardError> {
        self.index.board_clean_apply_atomic(token, include_stale)
    }

    pub fn board_update_files(
        &self,
        ticket_id: &Uuid,
        agent_id: &str,
        add: Vec<String>,
        remove: Vec<String>,
    ) -> Result<BoardEntry, BoardError> {
        self.index
            .board_update_files_atomic(*ticket_id, agent_id, add, remove)
    }

    pub fn board_rename_file(
        &self,
        ticket_id: &Uuid,
        agent_id: &str,
        old_path: &str,
        new_path: &str,
    ) -> Result<BoardEntry, BoardError> {
        self.index
            .board_rename_file_atomic(*ticket_id, agent_id, old_path, new_path)
    }

    pub(super) fn board_reconcile(
        &self,
        ticket_id: &Uuid,
        is_revert: bool,
    ) {
        if is_revert {
            self.log_revert_reconcile_warning(ticket_id);
            return;
        }

        let Some(is_terminal) = self.ticket_is_terminal(ticket_id) else {
            return;
        };
        if is_terminal {
            self.complete_active_board_entries(ticket_id);
        }
    }

    fn log_revert_reconcile_warning(
        &self,
        ticket_id: &Uuid,
    ) {
        match self.index.board_find_active_for_ticket(*ticket_id) {
            Ok(Some((entry, _))) => {
                let state = self
                    .index
                    .get_ticket(ticket_id)
                    .ok()
                    .flatten()
                    .and_then(|ticket| ticket.state)
                    .unwrap_or_default();
                tracing::warn!(
                    ticket_id = %ticket_id,
                    entry_id = %entry.entry_id,
                    current_state = %state,
                    "board_reconcile: stale intent — ticket reverted but active board entry remains"
                );
            },
            Ok(None) => {},
            Err(error) => {
                tracing::warn!(
                    ticket_id = %ticket_id,
                    error = %error,
                    "board_reconcile: failed to look up active entry during revert"
                );
            },
        }
    }

    fn ticket_is_terminal(
        &self,
        ticket_id: &Uuid,
    ) -> Option<bool> {
        match self.index.get_ticket(ticket_id) {
            Ok(Some(indexed)) =>
                Some(self.indexed_ticket_is_terminal(&indexed)),
            Ok(None) => Some(true),
            Err(error) => {
                tracing::warn!(
                    ticket_id = %ticket_id,
                    error = %error,
                    "board_reconcile: failed to fetch ticket — skipping"
                );
                None
            },
        }
    }

    fn indexed_ticket_is_terminal(
        &self,
        indexed: &IndexedTicket,
    ) -> bool {
        let current_state = indexed.state.as_deref().unwrap_or("");
        self.schema_registry
            .get(&indexed.type_id)
            .is_some_and(|schema| {
                schema.terminal_states.contains(&current_state.to_string())
                    || !schema
                        .transitions
                        .iter()
                        .any(|transition| transition.from == current_state)
            })
    }

    fn complete_active_board_entries(
        &self,
        ticket_id: &Uuid,
    ) {
        match self.index.board_complete_all_for_ticket(*ticket_id) {
            Ok(entry_ids) if !entry_ids.is_empty() => {
                tracing::debug!(
                    ticket_id = %ticket_id,
                    completed_entries = ?entry_ids,
                    action = ?ReconcileAction::MarkedCompleted { entry_id: entry_ids[0] },
                    "board_reconcile: marked active entries completed"
                );
            },
            Ok(_) => {},
            Err(error) => {
                tracing::warn!(
                    ticket_id = %ticket_id,
                    error = %error,
                    "board_reconcile: failed to complete board entries"
                );
            },
        }
    }
}
