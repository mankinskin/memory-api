use super::*;

fn sequential_move_can_execute_without_commit_between_moves() {
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
            Some("planned"),
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
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let mut first_plan = source_store
        .plan_move_preflight(&first, &nested_repo)
        .unwrap();
    let second_plan = source_store
        .plan_move_preflight(&second, &nested_repo)
        .unwrap();

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

    first_plan = source_store
        .plan_move_preflight(&first, &nested_repo)
        .unwrap();
    first_plan.blockers.retain(|blocker| {
        !matches!(
            blocker,
            MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                | MovePreflightBlocker::DirtyTrackedFiles { .. }
        )
    });

    let first_destination_rel = first_plan
        .destination_entity_path
        .strip_prefix(&repo)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");

    let _first_outcome =
        source_store.execute_move_with_journal(&first_plan).unwrap();

    let second_after_first = source_store
        .plan_move_preflight(&second, &nested_repo)
        .unwrap();
    assert!(!second_after_first.blockers.iter().any(|blocker| matches!(
        blocker,
        MovePreflightBlocker::DirtyTrackedFiles { .. }
    )));

    let second_destination_rel = second_after_first
        .destination_entity_path
        .strip_prefix(&repo)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let second_outcome = source_store
        .execute_move_with_journal(&second_after_first)
        .unwrap();
    assert_eq!(second_outcome.journal.phase, MoveExecutionPhase::Validated);

    let updated_doc = std::fs::read_to_string(&doc_path).unwrap();
    assert!(updated_doc.contains(&first_destination_rel));
    assert!(updated_doc.contains(&second_destination_rel));
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
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let target_indexed = target_store.get_indexed(&id).unwrap().unwrap();

    let poisoned_index =
        RedbIndexStore::open(&source_store.index_root.join("tickets.db"))
            .unwrap();
    poisoned_index.insert_ticket(&target_indexed).unwrap();

    let domain = TicketMoveDomain::new(&source_store);
    assert!(
        !move_kernel::MoveDomain::entity_indexed_in(&domain, &repo, &id)
            .unwrap()
    );
    assert!(
        move_kernel::MoveDomain::entity_indexed_in(&domain, &nested_repo, &id)
            .unwrap()
    );
}

#[test]
fn sequential_move_after_resumed_execution_can_continue_without_clean_commit() {
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
            Some("planned"),
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
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let mut first_plan = source_store
        .plan_move_preflight(&first, &nested_repo)
        .unwrap();
    let second_plan = source_store
        .plan_move_preflight(&second, &nested_repo)
        .unwrap();

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

    first_plan = source_store
        .plan_move_preflight(&first, &nested_repo)
        .unwrap();
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
        rollback_steps: vec![
            "rename destination entity folder back to source path".to_string(),
        ],
        lock_paths: move_kernel::collect_lock_paths(
            first,
            &first_plan.source_store_root,
            &first_plan.target_store_root,
        ),
        migrated_board_entries: Vec::new(),
        rewritten_path_files: Vec::new(),
        manual_followups: Vec::new(),
        phase_timings_ms: Default::default(),
        failure: None,
        next_recovery_step: None,
    };
    move_kernel::persist_journal(&first_plan.source_store_root, &journal)
        .unwrap();

    let resumed = source_store.resume_move_with_journal(journal_id).unwrap();
    assert!(resumed.resumed);
    assert_eq!(resumed.journal.phase, MoveExecutionPhase::Validated);

    let second_after_resume = source_store
        .plan_move_preflight(&second, &nested_repo)
        .unwrap();
    assert!(!second_after_resume.blockers.iter().any(|blocker| matches!(
        blocker,
        MovePreflightBlocker::DirtyTrackedFiles { .. }
    )));

    let continued = source_store
        .execute_move_with_journal(&second_after_resume)
        .unwrap();
    assert_eq!(continued.journal.phase, MoveExecutionPhase::Validated);
}

#[test]
fn resume_move_normalizes_journal_paths_before_validation() {
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

    let id = source_store
        .create(
            None,
            "tracker-improvement",
            Some("resume normalized paths"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let plan = source_store.plan_move_preflight(&id, &nested_repo).unwrap();
    std::fs::create_dir_all(plan.destination_entity_path.parent().unwrap())
        .unwrap();
    std::fs::rename(&plan.source_entity_path, &plan.destination_entity_path)
        .unwrap();
    let source_domain = TicketMoveDomain::new(&source_store);
    let target_domain = TicketMoveDomain::new(&target_store);
    move_kernel::MoveDomain::reconcile_store_touched(
        &source_domain,
        &plan.source_store_root,
        &[id],
    )
    .unwrap();
    move_kernel::MoveDomain::reconcile_store_touched(
        &target_domain,
        &plan.target_store_root,
        &[id],
    )
    .unwrap();

    let journal_id = Uuid::new_v4();
    let journal = MoveJournal {
        id: journal_id,
        entity_id: id,
        source_store_root: plan.source_store_root.clone(),
        target_store_root: plan.target_store_root.clone(),
        source_entity_path: plan.destination_entity_path.clone(),
        destination_entity_path: plan.destination_entity_path.clone(),
        phase: MoveExecutionPhase::TargetScanned,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        steps: vec![
            "created move journal".to_string(),
            "acquired source/target store locks and move entity lock"
                .to_string(),
            "moved entity folder".to_string(),
            "scanned source store".to_string(),
            "scanned target store".to_string(),
        ],
        rollback_steps: vec![
            "rename destination entity folder back to source path".to_string(),
        ],
        lock_paths: move_kernel::collect_lock_paths(
            id,
            &plan.source_store_root,
            &plan.target_store_root,
        ),
        migrated_board_entries: Vec::new(),
        rewritten_path_files: Vec::new(),
        manual_followups: Vec::new(),
        phase_timings_ms: Default::default(),
        failure: Some("injected stale source path".to_string()),
        next_recovery_step: Some(
            "run rollback_move for safety, or resume_move to retry".to_string(),
        ),
    };
    move_kernel::persist_journal(&plan.source_store_root, &journal).unwrap();

    let resumed = source_store.resume_move_with_journal(journal_id).unwrap();
    assert!(resumed.resumed);
    assert_eq!(resumed.journal.phase, MoveExecutionPhase::Validated);
    assert_eq!(resumed.journal.source_entity_path, plan.source_entity_path);
    assert_eq!(
        resumed.journal.destination_entity_path,
        plan.destination_entity_path
    );
}

#[test]
fn move_rewrites_skip_generated_store_indexes_and_journals() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    let nested_repo = repo.join("nested-repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&nested_repo).unwrap();
    run_git(&repo, &["init"]);
    run_git(&nested_repo, &["init"]);

    let source_store = TicketStore::init(&repo).unwrap();
    let _target_store = TicketStore::init(&nested_repo).unwrap();

    let id = source_store
        .create(
            None,
            "tracker-improvement",
            Some("move me"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let related = source_store
        .create(
            None,
            "tracker-improvement",
            Some("related ticket"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let mut plan = source_store.plan_move_preflight(&id, &nested_repo).unwrap();
    let source_rel = plan
        .source_entity_path
        .strip_prefix(&repo)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let source_abs =
        plan.source_entity_path.to_string_lossy().replace('\\', "/");

    let persistent_doc = repo
        .join(".ticket")
        .join("tickets")
        .join(related.to_string())
        .join("description.md");
    std::fs::create_dir_all(persistent_doc.parent().unwrap()).unwrap();
    std::fs::write(
            &persistent_doc,
            format!("persistent rel ref: {source_rel}\npersistent abs ref: {source_abs}\n"),
        )
        .unwrap();

    let generated_readme = repo.join(".ticket").join("README.md");
    std::fs::create_dir_all(generated_readme.parent().unwrap()).unwrap();
    std::fs::write(&generated_readme, format!("generated ref: {source_rel}\n"))
        .unwrap();

    let generated_index = repo.join(".ticket").join("index.toon");
    std::fs::write(
        &generated_index,
        format!("source_path: \"{source_rel}\"\n"),
    )
    .unwrap();

    let generated_journal = repo
        .join(".ticket")
        .join("move-journals")
        .join("existing.json");
    std::fs::create_dir_all(generated_journal.parent().unwrap()).unwrap();
    std::fs::write(
        &generated_journal,
        format!("{{\"ref\":\"{source_rel}\"}}\n"),
    )
    .unwrap();

    run_git(&repo, &["config", "user.name", "Move Test"]);
    run_git(&repo, &["config", "user.email", "move-test@example.com"]);
    run_git(
        &repo,
        &[
            "add",
            "--",
            &format!(".ticket/tickets/{}/description.md", related),
            ".ticket/README.md",
            ".ticket/index.toon",
            ".ticket/move-journals/existing.json",
        ],
    );
    run_git(
        &repo,
        &["commit", "-m", "seed persistent and generated refs"],
    );

    plan = source_store.plan_move_preflight(&id, &nested_repo).unwrap();
    assert!(
        !plan
            .path_reference_files
            .iter()
            .any(|path| path == &generated_readme)
    );
    assert!(
        !plan
            .path_reference_files
            .iter()
            .any(|path| path == &generated_index)
    );
    assert!(
        !plan
            .path_reference_files
            .iter()
            .any(|path| path == &generated_journal)
    );
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
            Some("planned"),
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
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let mut first_plan = source_store
        .plan_move_preflight(&first, &nested_repo)
        .unwrap();
    let second_plan = source_store
        .plan_move_preflight(&second, &nested_repo)
        .unwrap();

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

    first_plan = source_store
        .plan_move_preflight(&first, &nested_repo)
        .unwrap();
    first_plan.blockers.retain(|blocker| {
        !matches!(
            blocker,
            MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                | MovePreflightBlocker::DirtyTrackedFiles { .. }
        )
    });

    let first_outcome =
        source_store.execute_move_with_journal(&first_plan).unwrap();
    let _rolled_back = source_store
        .rollback_move_with_journal(first_outcome.journal.id)
        .unwrap();

    let second_after_rollback = source_store
        .plan_move_preflight(&second, &nested_repo)
        .unwrap();
    assert!(
        !second_after_rollback
            .blockers
            .iter()
            .any(|blocker| matches!(
                blocker,
                MovePreflightBlocker::DirtyTrackedFiles { .. }
            ))
    );
}

#[test]
fn rollback_of_second_move_preserves_first_move_rewrites() {
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
            Some("planned"),
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
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let mut first_plan = source_store
        .plan_move_preflight(&first, &nested_repo)
        .unwrap();
    let second_plan = source_store
        .plan_move_preflight(&second, &nested_repo)
        .unwrap();

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
    let doc_path = repo.join("docs").join("shared-spec-sequential-rollback.md");
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
        "docs/shared-spec-sequential-rollback.md",
        "seed shared refs for sequential rollback",
    );

    first_plan = source_store
        .plan_move_preflight(&first, &nested_repo)
        .unwrap();
    first_plan.blockers.retain(|blocker| {
        !matches!(
            blocker,
            MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                | MovePreflightBlocker::DirtyTrackedFiles { .. }
        )
    });
    let first_destination_rel = first_plan
        .destination_entity_path
        .strip_prefix(&repo)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let first_outcome =
        source_store.execute_move_with_journal(&first_plan).unwrap();

    let second_after_first = source_store
        .plan_move_preflight(&second, &nested_repo)
        .unwrap();
    let second_destination_rel = second_after_first
        .destination_entity_path
        .strip_prefix(&repo)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let second_outcome = source_store
        .execute_move_with_journal(&second_after_first)
        .unwrap();

    let rewritten_doc = std::fs::read_to_string(&doc_path).unwrap();
    assert!(rewritten_doc.contains(&first_destination_rel));
    assert!(rewritten_doc.contains(&second_destination_rel));

    let _rolled_back = source_store
        .rollback_move_with_journal(second_outcome.journal.id)
        .unwrap();
    let restored_doc = std::fs::read_to_string(&doc_path).unwrap();
    assert!(restored_doc.contains(&first_destination_rel));
    assert!(restored_doc.contains(&second_source_rel));
    assert!(!restored_doc.contains(&second_destination_rel));

    let first_ticket = TicketStore::open(&nested_repo)
        .unwrap()
        .get_indexed(&first)
        .unwrap();
    assert!(first_ticket.is_some());
    let second_ticket = TicketStore::open(&repo)
        .unwrap()
        .get_indexed(&second)
        .unwrap();
    assert!(second_ticket.is_some());
    let _ = first_outcome;
}
