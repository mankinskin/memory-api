use std::time::Instant;

use chrono::Utc;
use memory_fixtures::{
    FixtureError,
    TicketPerfFixtureOptions,
    append_fixture_ticket,
    materialize_fixture_with_ticket_perf_load,
    materialize_git_fixture_with_ticket_perf_load,
};
use ticket_api::{
    health::collect_findings,
    model::edge::EdgeRecord,
    storage::{
        move_execution::MoveExecutionPhase,
        move_planner::MovePreflightBlocker,
        store::TicketStore,
    },
    workflow::WorkflowModel,
};
use uuid::Uuid;

fn git_available_or_skip(
    result: Result<memory_fixtures::TicketPerfFixture, FixtureError>,
) -> Option<memory_fixtures::TicketPerfFixture> {
    match result {
        Ok(fixture) => Some(fixture),
        Err(FixtureError::Git { detail, .. }) if detail.contains("os error 2") => None,
        Err(err) => panic!("git perf fixture should materialize: {err}"),
    }
}

fn parse_ids(ids: &[String]) -> Vec<Uuid> {
    ids.iter().map(|id| id.parse().expect("valid fixture uuid")).collect()
}

fn add_perf_edges(store: &TicketStore, ids: &[Uuid]) {
    let now = Utc::now();
    for pair in ids.windows(2) {
        store
            .add_edge(EdgeRecord {
                from: pair[0],
                to: pair[1],
                kind: "depends_on".to_string(),
                created_at: now,
            })
            .expect("add chain edge");
    }

    let fanout = ids.len().min(24);
    if fanout > 1 {
        let root = ids[0];
        for id in &ids[1..fanout] {
            store
                .add_edge(EdgeRecord {
                    from: root,
                    to: *id,
                    kind: "linked".to_string(),
                    created_at: now,
                })
                .expect("add linked edge");
        }
    }
}

#[test]
fn reference_heavy_move_e2e_reports_timings() {
    let Some(perf) = git_available_or_skip(materialize_git_fixture_with_ticket_perf_load(
        TicketPerfFixtureOptions {
            root_generated_ticket_count: 64,
            submodule_generated_ticket_count: 40,
            tracked_reference_file_count: 10,
            references_per_file: 18,
        },
    )) else {
        eprintln!("git not available; skipping perf move E2E");
        return;
    };

    let source_root = perf
        .fixture
        .store_root("ticket-submodule-a")
        .expect("submodule ticket store path")
        .to_path_buf();
    let target_workspace = perf.fixture.workspace_root.clone();
    let source_store = TicketStore::open_or_init(&source_root).expect("open source store");
    source_store.scan(true).expect("scan source");
    let target_store = TicketStore::open_or_init(&target_workspace).expect("open target store");
    target_store.scan(true).expect("scan target");

    let id: Uuid = perf.submodule_ticket_ids[0].parse().expect("fixture move id");

    let preflight_started = Instant::now();
    let mut plan = source_store
        .plan_move_preflight(&id, &target_workspace)
        .expect("plan move preflight");
    let preflight_elapsed = preflight_started.elapsed();
    plan.blockers.retain(|blocker| {
        !matches!(
            blocker,
            MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                | MovePreflightBlocker::DirtyTrackedFiles { .. }
        )
    });
    assert!(plan.supported());
    assert!(!plan.path_reference_files.is_empty());

    let execute_started = Instant::now();
    let outcome = source_store
        .execute_move_with_journal(&plan)
        .expect("execute move");
    let execute_elapsed = execute_started.elapsed();
    assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);
    assert!(!outcome.journal.rewritten_path_files.is_empty());
    assert!(outcome.journal.phase_timings_ms.contains_key("rewrite_path_refs_ms"));
    assert!(outcome.journal.phase_timings_ms.contains_key("scan_source_ms"));
    assert!(outcome.journal.phase_timings_ms.contains_key("scan_target_ms"));

    let rollback_started = Instant::now();
    let rolled_back = source_store
        .rollback_move_with_journal(outcome.journal.id)
        .expect("rollback move");
    let rollback_elapsed = rollback_started.elapsed();
    assert!(rolled_back.rolled_back);
    assert_eq!(rolled_back.journal.phase, MoveExecutionPhase::RolledBack);

    eprintln!(
        "move_perf preflight_ms={} execute_ms={} rollback_ms={} refs={} rewrites={} phases={:?}",
        preflight_elapsed.as_millis(),
        execute_elapsed.as_millis(),
        rollback_elapsed.as_millis(),
        plan.path_reference_files.len(),
        outcome.journal.rewritten_path_files.len(),
        outcome.journal.phase_timings_ms,
    );
}

#[test]
fn reference_heavy_move_missing_tracked_file_records_followup_with_timing() {
    let Some(perf) = git_available_or_skip(materialize_git_fixture_with_ticket_perf_load(
        TicketPerfFixtureOptions {
            root_generated_ticket_count: 48,
            submodule_generated_ticket_count: 24,
            tracked_reference_file_count: 8,
            references_per_file: 14,
        },
    )) else {
        eprintln!("git not available; skipping missing-file perf move E2E");
        return;
    };

    let source_root = perf
        .fixture
        .store_root("ticket-submodule-a")
        .expect("submodule ticket store path")
        .to_path_buf();
    let target_workspace = perf.fixture.workspace_root.clone();
    let source_store = TicketStore::open_or_init(&source_root).expect("open source store");
    source_store.scan(true).expect("scan source");
    let target_store = TicketStore::open_or_init(&target_workspace).expect("open target store");
    target_store.scan(true).expect("scan target");

    let id: Uuid = perf.submodule_ticket_ids[1].parse().expect("fixture move id");
    let mut plan = source_store
        .plan_move_preflight(&id, &target_workspace)
        .expect("plan move preflight");
    plan.blockers.retain(|blocker| {
        !matches!(
            blocker,
            MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                | MovePreflightBlocker::DirtyTrackedFiles { .. }
        )
    });
    assert!(plan.supported());

    let missing_file = plan.path_reference_files[0].clone();
    std::fs::remove_file(&missing_file).expect("remove tracked reference file");

    let execute_started = Instant::now();
    let outcome = source_store
        .execute_move_with_journal(&plan)
        .expect("execute move with missing tracked reference file");
    let execute_elapsed = execute_started.elapsed();
    assert!(outcome
        .journal
        .manual_followups
        .iter()
        .any(|followup| followup.path == missing_file));

    eprintln!(
        "move_missing_ref_perf execute_ms={} refs={} followups={}",
        execute_elapsed.as_millis(),
        plan.path_reference_files.len(),
        outcome.journal.manual_followups.len(),
    );
}

#[test]
fn health_all_e2e_reports_timings_on_large_fixture() {
    let perf = materialize_fixture_with_ticket_perf_load(TicketPerfFixtureOptions {
        root_generated_ticket_count: 96,
        submodule_generated_ticket_count: 24,
        tracked_reference_file_count: 4,
        references_per_file: 10,
    })
    .expect("perf fixture should materialize");

    let root_store = perf
        .fixture
        .store_root("ticket-root")
        .expect("root ticket store path")
        .to_path_buf();

    let open_started = Instant::now();
    let store = TicketStore::open_or_init(&root_store).expect("open root store");
    let open_elapsed = open_started.elapsed();

    let scan_started = Instant::now();
    store.scan(true).expect("scan root store");
    let scan_elapsed = scan_started.elapsed();

    append_fixture_ticket(
        &root_store,
        "00000000-0000-5000-0000-000000000001",
        "incremental perf fixture ticket",
        "ready",
        "perf",
    )
    .expect("append incremental fixture ticket");
    let incremental_scan_started = Instant::now();
    store.scan(false).expect("incremental scan root store");
    let incremental_scan_elapsed = incremental_scan_started.elapsed();

    let ids = parse_ids(&perf.root_ticket_ids);
    add_perf_edges(&store, &ids);

    let list_started = Instant::now();
    let tickets = store.list(None, None, None).expect("list tickets");
    let all_edges = store.list_all_edges().expect("list edges");
    let list_elapsed = list_started.elapsed();

    let workflow_started = Instant::now();
    let workflow = WorkflowModel::build(&store, tickets.clone(), all_edges.clone())
        .expect("build workflow");
    let workflow_elapsed = workflow_started.elapsed();

    let health_started = Instant::now();
    let report = collect_findings(&store, &tickets, &all_edges, &workflow);
    let health_elapsed = health_started.elapsed();

    assert!(!report.findings.is_empty());
    assert!(report.summary.contains_key("graph_participation") || report.summary.contains_key("missing_effort_estimation"));

    eprintln!(
        "health_perf open_ms={} scan_true_ms={} scan_false_ms={} list_ms={} workflow_ms={} collect_ms={} tickets={} edges={} findings={}",
        open_elapsed.as_millis(),
        scan_elapsed.as_millis(),
        incremental_scan_elapsed.as_millis(),
        list_elapsed.as_millis(),
        workflow_elapsed.as_millis(),
        health_elapsed.as_millis(),
        tickets.len(),
        all_edges.len(),
        report.findings.len(),
    );
}

#[test]
fn stress_reference_heavy_sequential_moves_report_timings() {
    let Some(perf) = git_available_or_skip(materialize_git_fixture_with_ticket_perf_load(
        TicketPerfFixtureOptions::heavy(),
    )) else {
        eprintln!("git not available; skipping sequential perf move E2E");
        return;
    };

    let source_root = perf
        .fixture
        .store_root("ticket-submodule-a")
        .expect("submodule ticket store path")
        .to_path_buf();
    let target_workspace = perf.fixture.workspace_root.clone();
    let source_store = TicketStore::open_or_init(&source_root).expect("open source store");
    source_store.scan(true).expect("scan source");
    let target_store = TicketStore::open_or_init(&target_workspace).expect("open target store");
    target_store.scan(true).expect("scan target");

    let first_id: Uuid = perf.submodule_ticket_ids[0].parse().expect("first move id");
    let second_id: Uuid = perf.submodule_ticket_ids[1].parse().expect("second move id");

    let mut first_plan = source_store
        .plan_move_preflight(&first_id, &target_workspace)
        .expect("plan first move");
    first_plan.blockers.retain(|blocker| {
        !matches!(
            blocker,
            MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                | MovePreflightBlocker::DirtyTrackedFiles { .. }
        )
    });
    let first_started = Instant::now();
    let first = source_store
        .execute_move_with_journal(&first_plan)
        .expect("execute first move");
    let first_elapsed = first_started.elapsed();

    let mut second_plan = source_store
        .plan_move_preflight(&second_id, &target_workspace)
        .expect("plan second move");
    second_plan.blockers.retain(|blocker| {
        !matches!(
            blocker,
            MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                | MovePreflightBlocker::DirtyTrackedFiles { .. }
        )
    });
    let second_started = Instant::now();
    let second = source_store
        .execute_move_with_journal(&second_plan)
        .expect("execute second move");
    let second_elapsed = second_started.elapsed();

    eprintln!(
        "move_seq_perf first_ms={} second_ms={} first_phases={:?} second_phases={:?}",
        first_elapsed.as_millis(),
        second_elapsed.as_millis(),
        first.journal.phase_timings_ms,
        second.journal.phase_timings_ms,
    );
}