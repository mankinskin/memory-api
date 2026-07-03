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
    use crate::{
        model::filesystem::ScanRoot,
        storage::{
            index::RedbIndexStore,
            move_planner::MovePreflightBlocker,
            BoardEntryStatus,
        },
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

    fn git_commit_path(
        repo_root: &std::path::Path,
        pathspec: &str,
        message: &str,
    ) {
        run_git(repo_root, &["config", "user.name", "Move Test"]);
        run_git(repo_root, &["config", "user.email", "move-test@example.com"]);
        run_git(repo_root, &["add", "--", pathspec]);
        run_git(repo_root, &["commit", "-m", message]);
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
        git_commit_path(&repo, "docs/ticket-path.md", "seed tracked ticket path ref");

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
        assert!(outcome
            .journal
            .rewritten_path_files
            .iter()
            .all(|rewrite| rewrite.previous_content.is_none()));
        let rewritten_doc = std::fs::read_to_string(&doc_path).unwrap();
        assert!(rewritten_doc.contains(&destination_rel));

        let journal_path = plan
            .source_store_root
            .join("move-journals")
            .join(format!("{}.json", outcome.journal.id));
        let journal_text = std::fs::read_to_string(journal_path).unwrap();
        assert!(!journal_text.contains("previous_content"));
        assert!(!journal_text.contains(r#"\\"#));

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

    #[test]
    fn resume_move_with_journal_recovers_after_injected_file_move_failure() {
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

        let executed = source_store.execute_move_with_journal(&plan).unwrap();
        let journal_id = Uuid::new_v4();
        let mut journal = executed.journal.clone();
        journal.id = journal_id;
        journal.phase = MoveExecutionPhase::Moved;
        journal.failure = Some("injected failure after file movement".to_string());
        journal.next_recovery_step = Some("resume or rollback".to_string());
        journal.updated_at = Utc::now();
        journal
            .steps
            .push("injected failure after file move".to_string());
        move_kernel::persist_journal(&plan.source_store_root, &journal).unwrap();

        let outcome = source_store.resume_move_with_journal(journal_id).unwrap();
        assert!(outcome.resumed);
        assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);

        let src = TicketStore::open(&source_workspace).unwrap();
        let dst = TicketStore::open(&target_workspace).unwrap();
        assert!(src.get_indexed(&id).unwrap().is_none());
        assert!(dst.get_indexed(&id).unwrap().is_some());
    }

    #[test]
    fn rollback_move_with_journal_recovers_after_injected_file_move_failure() {
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

        let executed = source_store.execute_move_with_journal(&plan).unwrap();
        let journal_id = Uuid::new_v4();
        let mut journal = executed.journal.clone();
        journal.id = journal_id;
        journal.phase = MoveExecutionPhase::Moved;
        journal.failure = Some("injected failure after file movement".to_string());
        journal.next_recovery_step = Some("resume or rollback".to_string());
        journal.updated_at = Utc::now();
        journal
            .steps
            .push("injected failure after file move".to_string());
        move_kernel::persist_journal(&plan.source_store_root, &journal).unwrap();

        let outcome = source_store.rollback_move_with_journal(journal_id).unwrap();
        assert!(outcome.rolled_back);

        let src = TicketStore::open(&source_workspace).unwrap();
        let dst = TicketStore::open(&target_workspace).unwrap();
        assert!(src.get_indexed(&id).unwrap().is_some());
        assert!(dst.get_indexed(&id).unwrap().is_none());
    }

    #[test]
    fn sequential_move_requires_commit_or_rollback_between_executes() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        let nested_repo = repo.join("nested-repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&nested_repo).unwrap();
        run_git(&repo, &["init"]);
        run_git(&nested_repo, &["init"]);

        let source_store = TicketStore::init(&repo).unwrap();
        let _target_store = TicketStore::init(&nested_repo).unwrap();

        let first = source_store
            .create(
                None,
                "tracker-improvement",
                Some("first move"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();
        let second = source_store
            .create(
                None,
                "tracker-improvement",
                Some("second move"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut first_plan = source_store.plan_move_preflight(&first, &nested_repo).unwrap();
        let second_plan = source_store.plan_move_preflight(&second, &nested_repo).unwrap();

        let first_source_rel = first_plan
            .source_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let second_source_rel = second_plan
            .source_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let doc_path = repo.join("docs").join("shared-spec.md");
        std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
        std::fs::write(
            &doc_path,
            format!(
                "first reference: {}\nsecond reference: {}\n",
                first_source_rel, second_source_rel
            ),
        )
        .unwrap();
        git_commit_path(&repo, "docs/shared-spec.md", "seed shared refs");

        first_plan = source_store.plan_move_preflight(&first, &nested_repo).unwrap();
        first_plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let _first_outcome = source_store.execute_move_with_journal(&first_plan).unwrap();

        let second_after_first = source_store.plan_move_preflight(&second, &nested_repo).unwrap();
        assert!(second_after_first.blockers.iter().any(|blocker| matches!(
            blocker,
            MovePreflightBlocker::DirtyTrackedFiles { files }
            if files.iter().any(|file| file.ends_with("docs/shared-spec.md"))
        )));
    }

    #[test]
    fn entity_indexed_in_requires_path_ownership_not_aggregate_visibility() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        let nested_repo = repo.join("nested-repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&nested_repo).unwrap();
        run_git(&repo, &["init"]);
        run_git(&nested_repo, &["init"]);

        let source_store = TicketStore::init(&repo).unwrap();
        let target_store = TicketStore::init(&nested_repo).unwrap();
        source_store
            .add_scan_root(ScanRoot {
                path: nested_repo.join(".ticket").join("tickets"),
                label: "nested-tickets".to_string(),
            })
            .unwrap();

        let id = target_store
            .create(
                None,
                "tracker-improvement",
                Some("nested visibility ticket"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let target_indexed = target_store.get_indexed(&id).unwrap().unwrap();

        let poisoned_index = RedbIndexStore::open(&source_store.index_root.join("tickets.db"))
            .unwrap();
        poisoned_index.insert_ticket(&target_indexed).unwrap();

        let domain = TicketMoveDomain::new(&source_store);
        assert!(!move_kernel::MoveDomain::entity_indexed_in(&domain, &repo, &id).unwrap());
        assert!(move_kernel::MoveDomain::entity_indexed_in(&domain, &nested_repo, &id).unwrap());
    }

    #[test]
    fn sequential_move_after_resumed_execution_is_blocked_until_clean() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        let nested_repo = repo.join("nested-repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&nested_repo).unwrap();
        run_git(&repo, &["init"]);
        run_git(&nested_repo, &["init"]);

        let source_store = TicketStore::init(&repo).unwrap();
        let _target_store = TicketStore::init(&nested_repo).unwrap();

        let first = source_store
            .create(
                None,
                "tracker-improvement",
                Some("first move"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();
        let second = source_store
            .create(
                None,
                "tracker-improvement",
                Some("second move"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut first_plan = source_store.plan_move_preflight(&first, &nested_repo).unwrap();
        let second_plan = source_store.plan_move_preflight(&second, &nested_repo).unwrap();

        let first_source_rel = first_plan
            .source_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let second_source_rel = second_plan
            .source_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let doc_path = repo.join("docs").join("shared-spec-resume.md");
        std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
        std::fs::write(
            &doc_path,
            format!(
                "first reference: {}\nsecond reference: {}\n",
                first_source_rel, second_source_rel
            ),
        )
        .unwrap();
        git_commit_path(
            &repo,
            "docs/shared-spec-resume.md",
            "seed shared refs for resume",
        );

        first_plan = source_store.plan_move_preflight(&first, &nested_repo).unwrap();
        first_plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let journal_id = Uuid::new_v4();
        let journal = MoveJournal {
            id: journal_id,
            entity_id: first,
            source_store_root: first_plan.source_store_root.clone(),
            target_store_root: first_plan.target_store_root.clone(),
            source_entity_path: first_plan.source_entity_path.clone(),
            destination_entity_path: first_plan.destination_entity_path.clone(),
            phase: MoveExecutionPhase::Locked,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            steps: vec!["created move journal".to_string()],
            rollback_steps: vec!["rename destination entity folder back to source path".to_string()],
            lock_paths: move_kernel::collect_lock_paths(
                first,
                &first_plan.source_store_root,
                &first_plan.target_store_root,
            ),
            migrated_board_entries: Vec::new(),
            rewritten_path_files: Vec::new(),
            manual_followups: Vec::new(),
            failure: None,
            next_recovery_step: None,
        };
        move_kernel::persist_journal(&first_plan.source_store_root, &journal).unwrap();

        let resumed = source_store.resume_move_with_journal(journal_id).unwrap();
        assert!(resumed.resumed);
        assert_eq!(resumed.journal.phase, MoveExecutionPhase::Validated);

        let second_after_resume = source_store.plan_move_preflight(&second, &nested_repo).unwrap();
        assert!(second_after_resume.blockers.iter().any(|blocker| matches!(
            blocker,
            MovePreflightBlocker::DirtyTrackedFiles { files }
            if files.iter().any(|file| file.ends_with("docs/shared-spec-resume.md"))
        )));
    }

    #[test]
    fn rollback_clears_rewrites_and_unblocks_next_sequential_move() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        let nested_repo = repo.join("nested-repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&nested_repo).unwrap();
        run_git(&repo, &["init"]);
        run_git(&nested_repo, &["init"]);

        let source_store = TicketStore::init(&repo).unwrap();
        let _target_store = TicketStore::init(&nested_repo).unwrap();

        let first = source_store
            .create(
                None,
                "tracker-improvement",
                Some("first move"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();
        let second = source_store
            .create(
                None,
                "tracker-improvement",
                Some("second move"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut first_plan = source_store.plan_move_preflight(&first, &nested_repo).unwrap();
        let second_plan = source_store.plan_move_preflight(&second, &nested_repo).unwrap();

        let first_source_rel = first_plan
            .source_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let second_source_rel = second_plan
            .source_entity_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let doc_path = repo.join("docs").join("shared-spec-rollback.md");
        std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
        std::fs::write(
            &doc_path,
            format!(
                "first reference: {}\nsecond reference: {}\n",
                first_source_rel, second_source_rel
            ),
        )
        .unwrap();
        git_commit_path(
            &repo,
            "docs/shared-spec-rollback.md",
            "seed shared refs for rollback",
        );

        first_plan = source_store.plan_move_preflight(&first, &nested_repo).unwrap();
        first_plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let first_outcome = source_store.execute_move_with_journal(&first_plan).unwrap();
        let _rolled_back = source_store
            .rollback_move_with_journal(first_outcome.journal.id)
            .unwrap();

        let second_after_rollback = source_store.plan_move_preflight(&second, &nested_repo).unwrap();
        assert!(!second_after_rollback.blockers.iter().any(|blocker| matches!(
            blocker,
            MovePreflightBlocker::DirtyTrackedFiles { .. }
        )));
    }
}
