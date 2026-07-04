//! Ticket-domain adapter onto the domain-neutral move kernel.
//!
//! The generic move machinery (preflight planning, journaled execution, git
//! topology, path-reference rewriting) lives in
//! [`memory_api::storage::move_kernel`]. This module supplies the ticket-domain
//! specialization via [`TicketMoveDomain`] and re-exports the kernel types under
//! the names the ticket surfaces (CLI/MCP/HTTP) consume.

use std::path::{
    Path,
    PathBuf,
};

use memory_api::storage::move_kernel::{
    self,
    MoveBoardState,
    MoveDomain,
    MoveError,
    MoveLeaseBlock,
    MoveReferences,
    MoveResult,
};
use uuid::Uuid;

use crate::{
    error::StorageError,
    storage::{
        store::TicketStore,
        BoardEntry,
        BoardEntryStatus,
    },
    workspace,
};

// Re-export the neutral kernel types as the ticket move surface. The move
// command is domain-neutral now; these aliases keep the existing public paths
// (`ticket_api::storage::move_planner::MovePreflightReport`, etc.) stable.
pub use memory_api::storage::move_kernel::{
    GitWorktreeTopology,
    MoveBlocker as MovePreflightBlocker,
    MovePlan as MovePreflightReport,
    MoveReferenceDirection,
    MoveReferenceVisibility,
};

/// Map a ticket [`StorageError`] into a kernel [`MoveError`].
fn to_move_error(error: StorageError) -> MoveError {
    match error {
        StorageError::Io(io) => MoveError::Io(io),
        other => MoveError::Domain(other.to_string()),
    }
}

/// Map a kernel [`MoveError`] back into a ticket [`StorageError`].
pub(crate) fn from_move_error(error: MoveError) -> StorageError {
    match error {
        MoveError::Io(io) => StorageError::Io(io),
        MoveError::Domain(message) => StorageError::Other(message),
    }
}

fn map_board_error(error: crate::storage::BoardError) -> MoveError {
    match error {
        crate::storage::BoardError::Storage(storage_error) => to_move_error(storage_error),
        other => MoveError::Domain(other.to_string()),
    }
}

fn ticket_entity_root(store_root: &Path) -> PathBuf {
    workspace::resolve_store_root_from(store_root, workspace::TICKET_INDEX_DIR).join("tickets")
}

/// Ticket-domain implementation of the move kernel's [`MoveDomain`] trait.
pub(crate) struct TicketMoveDomain<'a> {
    store: &'a TicketStore,
}

impl<'a> TicketMoveDomain<'a> {
    pub(crate) fn new(store: &'a TicketStore) -> Self {
        Self { store }
    }

    fn open(
        &self,
        store_root: &Path,
    ) -> MoveResult<TicketStore> {
        TicketStore::open_with(store_root, self.store.schema_registry().clone()).map_err(to_move_error)
    }
}

impl MoveDomain for TicketMoveDomain<'_> {
    fn entity_subdir(&self) -> &str {
        "tickets"
    }

    fn store_index_dir(&self) -> &str {
        workspace::TICKET_INDEX_DIR
    }

    fn source_store_root(&self) -> PathBuf {
        self.store.index_root.clone()
    }

    fn source_entity_path(
        &self,
        entity_id: &Uuid,
    ) -> MoveResult<Option<PathBuf>> {
        Ok(self
            .store
            .get_indexed(entity_id)
            .map_err(to_move_error)?
            .map(|ticket| ticket.path))
    }

    fn related_entities(
        &self,
        entity_id: &Uuid,
    ) -> MoveResult<MoveReferences> {
        let mut references = MoveReferences::default();
        for edge in self.store.list_all_edges().map_err(to_move_error)? {
            if edge.from == *entity_id {
                references.outbound.push(edge.to);
            }
            if edge.to == *entity_id {
                references.inbound.push(edge.from);
            }
        }
        Ok(references)
    }

    fn target_store_present(
        &self,
        target_store_root: &Path,
    ) -> MoveResult<bool> {
        match TicketStore::open_with(target_store_root, self.store.schema_registry().clone()) {
            Ok(_) => Ok(true),
            Err(StorageError::WorkspaceNotFound { .. }) => Ok(false),
            Err(error) => Err(to_move_error(error)),
        }
    }

    fn entity_indexed_in(
        &self,
        store_root: &Path,
        entity_id: &Uuid,
    ) -> MoveResult<bool> {
        let store = self.open(store_root)?;
        let entity_root = ticket_entity_root(store_root);
        Ok(store
            .get_indexed(entity_id)
            .map_err(to_move_error)?
            .map(|ticket| ticket.path.starts_with(&entity_root))
            .unwrap_or(false))
    }

    fn board_state(
        &self,
        entity_id: &Uuid,
    ) -> MoveResult<MoveBoardState> {
        let mut state = MoveBoardState::default();
        let snapshot = self.store.board_show(None).map_err(map_board_error)?;
        for entry in snapshot.entries {
            if entry.ticket_id == *entity_id
                && (entry.status == BoardEntryStatus::Active
                    || entry.status == BoardEntryStatus::Stale)
            {
                state.active_entries.push(entry);
            }
        }
        let history = self.store.board_history(None).map_err(map_board_error)?;
        for entry in history.entries {
            if entry.ticket_id == *entity_id {
                state.historical_entries.push(entry);
            }
        }
        Ok(state)
    }

    fn active_leases(
        &self,
        entity_id: &Uuid,
    ) -> MoveResult<Vec<MoveLeaseBlock>> {
        let mut leases = Vec::new();
        for lease in self.store.list_leases().map_err(to_move_error)? {
            if lease.ticket_id == *entity_id {
                leases.push(MoveLeaseBlock {
                    entity_id: lease.ticket_id,
                    working_by: lease.working_by.clone(),
                });
            }
        }
        Ok(leases)
    }

    fn migrate_board_history(
        &self,
        target_store_root: &Path,
        entity_id: &Uuid,
    ) -> MoveResult<Vec<BoardEntry>> {
        let target_store = self.open(target_store_root)?;
        let entries = self
            .store
            .board_list_entries_for_ticket(entity_id)
            .map_err(map_board_error)?;

        let mut historical_entries = Vec::new();
        for entry in entries {
            if entry.status == BoardEntryStatus::Active || entry.status == BoardEntryStatus::Stale {
                return Err(MoveError::Domain(format!(
                    "cannot move entity {} while board entry {} is active/stale",
                    entity_id, entry.entry_id
                )));
            }
            historical_entries.push(entry);
        }

        if historical_entries.is_empty() {
            return Ok(Vec::new());
        }

        target_store
            .board_import_entries(&historical_entries)
            .map_err(map_board_error)?;
        let ids: Vec<Uuid> = historical_entries.iter().map(|entry| entry.entry_id).collect();
        self.store
            .board_delete_entries(&ids)
            .map_err(map_board_error)?;

        Ok(historical_entries)
    }

    fn restore_board_history(
        &self,
        target_store_root: &Path,
        entries: &[BoardEntry],
    ) -> MoveResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let target_store = self.open(target_store_root)?;
        self.store
            .board_import_entries(entries)
            .map_err(map_board_error)?;
        let ids: Vec<Uuid> = entries.iter().map(|entry| entry.entry_id).collect();
        target_store
            .board_delete_entries(&ids)
            .map_err(map_board_error)?;
        Ok(())
    }

    fn scan_store(
        &self,
        store_root: &Path,
    ) -> MoveResult<()> {
        let store = self.open(store_root)?;
        store.scan(true).map_err(to_move_error)?;
        Ok(())
    }
}

impl TicketStore {
    /// Build a read-only preflight plan for moving `ticket_id` to
    /// `target_workspace_root`, delegating to the domain-neutral move kernel.
    pub fn plan_move_preflight(
        &self,
        ticket_id: &Uuid,
        target_workspace_root: &Path,
    ) -> Result<MovePreflightReport, StorageError> {
        let domain = TicketMoveDomain::new(self);
        move_kernel::plan_move(&domain, ticket_id, target_workspace_root).map_err(from_move_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::edge::EdgeRecord;
    use chrono::Utc;
    use std::{fs, process::Command};
    use tempfile::tempdir;

    fn run_git(repo_root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git {args:?} failed: {status}");
    }

    #[test]
    fn preflight_reports_invisible_reference_visibility_and_path_refs() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source-workspace");
        let target_workspace = repo.join("target-workspace");
        let docs_dir = repo.join("docs");
        fs::create_dir_all(&source_workspace).unwrap();
        fs::create_dir_all(&target_workspace).unwrap();
        fs::create_dir_all(&docs_dir).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let target_store = TicketStore::init(&target_workspace).unwrap();

        let source_ticket = source_store
            .create(
                None,
                "tracker-improvement",
                Some("source ticket"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();
        let invisible_inbound = source_store
            .create(
                None,
                "tracker-improvement",
                Some("invisible inbound"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let destination_visible = target_store
            .create(
                None,
                "tracker-improvement",
                Some("destination visible"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        source_store
            .add_edge(EdgeRecord {
                from: invisible_inbound,
                to: source_ticket,
                kind: "depends_on".to_string(),
                created_at: Utc::now(),
            })
            .unwrap();
        source_store
            .add_edge(EdgeRecord {
                from: source_ticket,
                to: destination_visible,
                kind: "depends_on".to_string(),
                created_at: Utc::now(),
            })
            .unwrap();

        let source_ticket_path = source_store
            .get_indexed(&source_ticket)
            .unwrap()
            .unwrap()
            .path;
        let relative_ticket_path = source_ticket_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let tracked_doc = docs_dir.join("move.md");
        fs::write(&tracked_doc, format!("See {relative_ticket_path}\n")).unwrap();
        run_git(&repo, &["add", "docs/move.md"]);

        let report = source_store
            .plan_move_preflight(&source_ticket, &target_workspace)
            .unwrap();

        assert!(report.supported());
        assert!(report.reference_visibility.iter().any(|entry| {
            entry.related_entity_id == invisible_inbound
                && entry.direction == MoveReferenceDirection::Inbound
                && !entry.visible_from_destination
        }));
        assert!(report.path_reference_files.iter().any(|path| {
            path.ends_with("docs/move.md")
        }));
        assert!(!report.blockers.iter().any(|blocker| matches!(
            blocker,
            MovePreflightBlocker::InvisibleReference { .. }
        )));
    }

    #[test]
    fn preflight_allows_parent_submodule_worktree_topology() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let nested_repo = repo.join("nested-repo");
        fs::create_dir_all(&nested_repo).unwrap();
        run_git(&nested_repo, &["init"]);

        let source_store = TicketStore::init(&repo).unwrap();
        let _target_store = TicketStore::init(&nested_repo).unwrap();

        let source_ticket = source_store
            .create(
                None,
                "tracker-improvement",
                Some("source ticket"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let report = source_store
            .plan_move_preflight(&source_ticket, &nested_repo)
            .unwrap();

        assert_eq!(report.git_worktree_topology, GitWorktreeTopology::ParentToSubmodule);
        assert!(!report.blockers.iter().any(|blocker| matches!(
            blocker,
            MovePreflightBlocker::DifferentGitWorktree { .. }
        )));
    }

    #[test]
    fn preflight_blocks_unrelated_git_worktrees() {
        let temp = tempdir().unwrap();
        let source_repo = temp.path().join("source-repo");
        let target_repo = temp.path().join("target-repo");
        fs::create_dir_all(&source_repo).unwrap();
        fs::create_dir_all(&target_repo).unwrap();
        run_git(&source_repo, &["init"]);
        run_git(&target_repo, &["init"]);

        let source_store = TicketStore::init(&source_repo).unwrap();
        let _target_store = TicketStore::init(&target_repo).unwrap();

        let source_ticket = source_store
            .create(
                None,
                "tracker-improvement",
                Some("source ticket"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let report = source_store
            .plan_move_preflight(&source_ticket, &target_repo)
            .unwrap();

        assert_eq!(report.git_worktree_topology, GitWorktreeTopology::Unrelated);
        assert!(report.blockers.iter().any(|blocker| matches!(
            blocker,
            MovePreflightBlocker::DifferentGitWorktree { .. }
        )));
    }
}
