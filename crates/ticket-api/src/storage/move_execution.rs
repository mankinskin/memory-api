//! Ticket-domain entry points for journaled cross-workspace moves.
//!
//! All execution logic lives in [`memory_api::storage::move_kernel`]; these
//! methods build a [`TicketMoveDomain`] adapter and delegate to the generic
//! kernel, mapping the kernel error back onto [`StorageError`]. The journal and
//! outcome types are re-exported so existing surfaces keep their public paths.

use uuid::Uuid;

use memory_api::storage::move_kernel;

use crate::{
    error::StorageError,
    storage::{
        move_planner::{
            from_move_error,
            MovePreflightReport,
            TicketMoveDomain,
        },
        store::TicketStore,
    },
};

// Re-export the neutral kernel execution types under their established paths.
pub use memory_api::storage::move_kernel::{
    MoveExecutionPhase,
    MoveJournal,
    MoveManualFollowup,
    MoveOutcome as MoveExecutionOutcome,
    MovePathRewrite,
};

impl TicketStore {
    pub fn execute_move_with_journal(
        &self,
        plan: &MovePreflightReport,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        let domain = TicketMoveDomain::new(self);
        move_kernel::execute_move(&domain, plan).map_err(from_move_error)
    }

    pub fn resume_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        let domain = TicketMoveDomain::new(self);
        move_kernel::resume_move(&domain, journal_id).map_err(from_move_error)
    }

    pub fn rollback_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        let domain = TicketMoveDomain::new(self);
        move_kernel::rollback_move(&domain, journal_id).map_err(from_move_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        move_planner::MovePreflightBlocker,
        BoardEntryStatus,
    };
    use chrono::Utc;
    use std::process::Command;
    use tempfile::tempdir;

    fn run_git(repo_root: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git {args:?} failed: {status}");
    }

    #[test]
    fn execute_move_with_journal_moves_ticket_between_stores() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);

        let src = TicketStore::open(&source_workspace).unwrap();
        let dst = TicketStore::open(&target_workspace).unwrap();
        assert!(src.get_indexed(&id).unwrap().is_none());
        assert!(dst.get_indexed(&id).unwrap().is_some());
    }

    #[test]
    fn rollback_move_with_journal_restores_source_location() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        let journal_id = outcome.journal.id;

        let _ = source_store.rollback_move_with_journal(journal_id).unwrap();

        let src = TicketStore::open(&source_workspace).unwrap();
        let dst = TicketStore::open(&target_workspace).unwrap();
        assert!(src.get_indexed(&id).unwrap().is_some());
        assert!(dst.get_indexed(&id).unwrap().is_none());
    }

    #[test]
    fn execute_move_with_journal_fails_when_board_entry_is_active() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        source_store
            .board_check_in(&id, "agent-a", 300, "working", Vec::new())
            .unwrap();

        let plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        assert!(!plan.supported());

        let error = source_store.execute_move_with_journal(&plan).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("move preflight contains blockers")
        );
        assert!(plan.source_entity_path.exists());
        assert!(!plan.destination_entity_path.exists());
    }

    #[test]
    fn execute_move_with_journal_migrates_historical_board_rows() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        source_store
            .board_check_in(&id, "agent-a", 300, "working", Vec::new())
            .unwrap();
        source_store
            .board_check_out(&id, "agent-a", Some("done"))
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);
        assert_eq!(outcome.journal.migrated_board_entries.len(), 1);

        let src_entries = source_store.board_list_entries_for_ticket(&id).unwrap();
        let dst_entries = target_store.board_list_entries_for_ticket(&id).unwrap();
        assert!(src_entries.is_empty());
        assert_eq!(dst_entries.len(), 1);
        assert_eq!(dst_entries[0].status, BoardEntryStatus::Completed);
    }

    #[test]
    fn execute_move_with_journal_rewrites_path_references_and_rollback_restores() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        let source_rel = plan
            .source_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let destination_rel = plan
            .destination_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let doc_path = repo.join("docs").join("ticket-path.md");
        std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
        std::fs::write(&doc_path, format!("ticket path: {}\n", source_rel)).unwrap();
        run_git(&repo, &["add", "docs/ticket-path.md"]);

        plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        assert!(!outcome.journal.rewritten_path_files.is_empty());
        let rewritten_doc = std::fs::read_to_string(&doc_path).unwrap();
        assert!(rewritten_doc.contains(&destination_rel));

        let _rollback = source_store
            .rollback_move_with_journal(outcome.journal.id)
            .unwrap();
        let restored_doc = std::fs::read_to_string(&doc_path).unwrap();
        assert!(restored_doc.contains(&source_rel));
    }

    #[test]
    fn execute_move_with_journal_rewrites_parent_repo_refs_for_submodule_source() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        let nested_repo = repo.join("nested-repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&nested_repo).unwrap();
        run_git(&repo, &["init"]);
        run_git(&nested_repo, &["init"]);

        let source_store = TicketStore::init(&nested_repo).unwrap();
        let _target_store = TicketStore::init(&repo).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store.plan_move_preflight(&id, &repo).unwrap();
        let source_rel_from_parent = plan
            .source_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let destination_rel_from_parent = plan
            .destination_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let doc_path = repo.join("docs").join("submodule-ticket-path.md");
        std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
        std::fs::write(
            &doc_path,
            format!("ticket path from parent repo: {}\n", source_rel_from_parent),
        )
        .unwrap();
        run_git(&repo, &["add", "docs/submodule-ticket-path.md"]);

        plan = source_store.plan_move_preflight(&id, &repo).unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        assert!(!outcome.journal.rewritten_path_files.is_empty());

        let rewritten_doc = std::fs::read_to_string(&doc_path).unwrap();
        assert!(rewritten_doc.contains(&destination_rel_from_parent));
        assert!(!rewritten_doc.contains(&source_rel_from_parent));
    }

    #[test]
    fn resume_move_with_journal_continues_from_locked_phase() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let source_ticket = plan.source_entity_path.clone();
        let destination_ticket = plan.destination_entity_path.clone();
        let journal_id = Uuid::new_v4();
        let lock_paths = move_kernel::collect_lock_paths(
            id,
            &plan.source_store_root,
            &plan.target_store_root,
        );
        let journal = MoveJournal {
            id: journal_id,
            entity_id: id,
            source_store_root: plan.source_store_root.clone(),
            target_store_root: plan.target_store_root.clone(),
            source_entity_path: source_ticket,
            destination_entity_path: destination_ticket,
            phase: MoveExecutionPhase::Locked,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            steps: vec!["created move journal".to_string()],
            rollback_steps: vec!["rename destination entity folder back to source path".to_string()],
            lock_paths,
            migrated_board_entries: Vec::new(),
            rewritten_path_files: Vec::new(),
            manual_followups: Vec::new(),
            failure: None,
            next_recovery_step: None,
        };
        move_kernel::persist_journal(&plan.source_store_root, &journal).unwrap();

        let outcome = source_store.resume_move_with_journal(journal_id).unwrap();
        assert!(outcome.resumed);
        assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);

        let src = TicketStore::open(&source_workspace).unwrap();
        let dst = TicketStore::open(&target_workspace).unwrap();
        assert!(src.get_indexed(&id).unwrap().is_none());
        assert!(dst.get_indexed(&id).unwrap().is_some());
    }
}
