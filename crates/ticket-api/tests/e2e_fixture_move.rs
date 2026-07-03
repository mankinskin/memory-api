//! E2E cross-worktree move test against the git-backed shared fixture.
//!
//! Materializes the multi-store fixture as real git repositories (root +
//! submodule worktrees) and exercises a parent↔submodule ticket move through
//! the journaled execution path. Asserts the destination path carries no
//! Windows verbatim prefix, the journal reaches the validated phase, the moved
//! ticket is readable from the destination store, and rollback restores it.

use std::fs;

use memory_fixtures::{FixtureError, materialize_git_fixture};
use ticket_api::storage::move_execution::MoveExecutionPhase;
use ticket_api::storage::move_planner::MovePreflightBlocker;
use ticket_api::storage::store::TicketStore;

fn git_available_or_skip(result: Result<memory_fixtures::LoadedFixture, FixtureError>) -> Option<memory_fixtures::LoadedFixture> {
    match result {
        Ok(fixture) => Some(fixture),
        Err(FixtureError::Git { detail, .. }) if detail.contains("os error 2") => None,
        Err(err) => panic!("git fixture should materialize: {err}"),
    }
}

#[test]
fn cross_worktree_move_from_submodule_to_root_is_clean_and_reversible() {
    let Some(fixture) = git_available_or_skip(materialize_git_fixture()) else {
        eprintln!("git not available; skipping cross-worktree move E2E");
        return;
    };

    let source_root = fixture
        .store_root("ticket-submodule-a")
        .expect("submodule ticket store path")
        .to_path_buf();
    let target_workspace = fixture.workspace_root.clone();

    // Ensure both stores are initialized (target store must exist for the move).
    let source_store = TicketStore::open_or_init(&source_root).expect("open source store");
    source_store.scan(true).expect("scan source");
    let target_store = TicketStore::open_or_init(&target_workspace).expect("open target store");
    target_store.scan(true).expect("scan target");

    let id = "00000000-0000-0000-0000-00000000000a"
        .parse()
        .expect("seeded submodule ticket id");

    let mut plan = source_store
        .plan_move_preflight(&id, &target_workspace)
        .expect("plan move preflight");

    // The fixture cannot pre-commit the freshly created ticket, and path-scan
    // may be unavailable in CI; drop only those non-topology blockers.
    plan.blockers.retain(|blocker| {
        !matches!(
            blocker,
            MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                | MovePreflightBlocker::DirtyTrackedFiles { .. }
        )
    });

    // This must be a genuine cross-worktree topology (parent vs. submodule).
    assert_ne!(
        plan.source_git_worktree_root, plan.target_git_worktree_root,
        "expected distinct git worktrees for parent↔submodule move"
    );
    let reference_file = fixture.workspace_root.join("submodule-a/README.md");
    assert!(
        plan.path_reference_files.iter().any(|path| path == &reference_file),
        "expected tracked submodule README path reference in preflight plan"
    );
    fs::remove_file(&reference_file).expect("remove tracked reference file");

    let outcome = source_store
        .execute_move_with_journal(&plan)
        .expect("execute move");

    // The destination path must never carry a Windows verbatim prefix.
    let dest = outcome
        .journal
        .destination_entity_path
        .to_string_lossy()
        .replace('\\', "/");
    assert!(!dest.contains("//?/"), "verbatim prefix leaked: {dest}");
    assert!(
        !outcome
            .journal
            .destination_entity_path
            .to_string_lossy()
            .contains(r"\\?\"),
        "verbatim prefix leaked into destination path"
    );

    assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);
    assert!(
        outcome
            .journal
            .manual_followups
            .iter()
            .any(|followup| followup.path == reference_file),
        "missing tracked reference file should be recorded for manual follow-up"
    );

    // Destination store can read the moved ticket.
    let target_store = TicketStore::open_or_init(&target_workspace).expect("reopen target");
    target_store.scan(true).expect("rescan target");
    assert!(
        target_store.get(&id).is_ok(),
        "moved ticket should be readable from destination store"
    );

    // Rollback restores the ticket to the source worktree.
    let rolled = source_store
        .rollback_move_with_journal(outcome.journal.id)
        .expect("rollback move");
    assert!(rolled.rolled_back);
    assert_eq!(rolled.journal.phase, MoveExecutionPhase::RolledBack);

    let source_store = TicketStore::open_or_init(&source_root).expect("reopen source");
    source_store.scan(true).expect("rescan source");
    assert!(
        source_store.get(&id).is_ok(),
        "ticket should be readable from source after rollback"
    );
}
