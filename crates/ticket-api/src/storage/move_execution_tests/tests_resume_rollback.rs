use super::*;

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
        rollback_steps: vec![
            "rename destination entity folder back to source path".to_string(),
        ],
        lock_paths,
        migrated_board_entries: Vec::new(),
        rewritten_path_files: Vec::new(),
        manual_followups: Vec::new(),
        phase_timings_ms: Default::default(),
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
