use std::{
    path::Path,
    sync::OnceLock,
    time::Instant,
};

use chrono::Utc;
use criterion::{
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use memory_fixtures::{
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

const PERF_TRACE_TARGET: &str = "ticket_api::perf";
static PERF_TRACING: OnceLock<()> = OnceLock::new();

fn init_perf_bench_tracing() {
    PERF_TRACING.get_or_init(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off"));
        let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    });
}

fn parse_ids(ids: &[String]) -> Vec<Uuid> {
    ids.iter()
        .map(|id| id.parse().expect("valid fixture uuid"))
        .collect()
}

fn add_perf_edges(
    store: &TicketStore,
    ids: &[Uuid],
) {
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

fn append_incremental_fixture_tickets(
    root_store: &Path,
    batch: usize,
    count: usize,
) {
    for offset in 0..count {
        let id = format!(
            "00000000-0000-5000-{batch:04x}-{value:012x}",
            value = offset + 1,
        );
        append_fixture_ticket(
            root_store,
            &id,
            &format!("bench incremental perf ticket {batch}-{offset}"),
            "planned",
            "perf",
        )
        .expect("append fixture ticket");
    }
}

fn percentile_nearest_rank(
    sorted: &[u64],
    percentile: u32,
) -> u64 {
    assert!(!sorted.is_empty(), "percentile input must not be empty");
    assert!(percentile <= 100, "percentile must be <= 100");

    if sorted.len() == 1 {
        return sorted[0];
    }

    let rank =
        ((percentile as f64 / 100.0) * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

fn percentile_summary(samples: &[u64]) -> (u64, u64, u64) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    (
        percentile_nearest_rank(&sorted, 50),
        percentile_nearest_rank(&sorted, 95),
        percentile_nearest_rank(&sorted, 99),
    )
}

fn map_percentiles(
    values: &std::collections::BTreeMap<String, u64>
) -> (u64, u64, u64) {
    let samples = values.values().copied().collect::<Vec<_>>();
    percentile_summary(&samples)
}

fn bench_move_preflight_reference_heavy(c: &mut Criterion) {
    init_perf_bench_tracing();
    c.bench_function("move_preflight_reference_heavy", |b| {
        b.iter_batched(
            || {
                let perf = materialize_git_fixture_with_ticket_perf_load(
                    TicketPerfFixtureOptions {
                        root_generated_ticket_count: 48,
                        submodule_generated_ticket_count: 24,
                        tracked_reference_file_count: 8,
                        references_per_file: 18,
                    },
                )
                .expect("perf fixture should materialize");
                let source_root = perf
                    .fixture
                    .store_root("ticket-submodule-a")
                    .expect("submodule store")
                    .to_path_buf();
                let target_workspace = perf.fixture.workspace_root.clone();
                let store = TicketStore::open_or_init(&source_root)
                    .expect("open source store");
                store.scan(true).expect("scan source store");
                let id: Uuid = perf.submodule_ticket_ids[0]
                    .parse()
                    .expect("fixture move id");
                (perf, store, target_workspace, id)
            },
            |(_perf, store, target_workspace, id)| {
                let started = Instant::now();
                let plan = store
                    .plan_move_preflight(&id, &target_workspace)
                    .expect("plan preflight");
                let elapsed = started.elapsed();
                criterion::black_box(elapsed);
                criterion::black_box(plan.path_reference_files.len());
                tracing::info!(
                    target: PERF_TRACE_TARGET,
                    benchmark = "move_preflight_reference_heavy",
                    run_id = %Uuid::new_v4(),
                    fixture_profile = "git_ref_heavy_48_24_8x18",
                    workspace_scope = "ticket-submodule-a",
                    change_count = 0,
                    reindex_mode = "true",
                    p50_ms = elapsed.as_millis() as u64,
                    p95_ms = elapsed.as_millis() as u64,
                    p99_ms = elapsed.as_millis() as u64,
                    tracked_reference_files = plan.path_reference_files.len(),
                    elapsed_ms = elapsed.as_millis() as u64,
                    "ticket_api_benchmark_iteration"
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_move_execute_reference_heavy(c: &mut Criterion) {
    init_perf_bench_tracing();
    c.bench_function("move_execute_reference_heavy", |b| {
        b.iter_batched(
            || {
                let perf = materialize_git_fixture_with_ticket_perf_load(TicketPerfFixtureOptions {
                    root_generated_ticket_count: 48,
                    submodule_generated_ticket_count: 24,
                    tracked_reference_file_count: 8,
                    references_per_file: 18,
                })
                .expect("perf fixture should materialize");
                let source_root = perf
                    .fixture
                    .store_root("ticket-submodule-a")
                    .expect("submodule store")
                    .to_path_buf();
                let target_workspace = perf.fixture.workspace_root.clone();
                let store = TicketStore::open_or_init(&source_root).expect("open source store");
                store.scan(true).expect("scan source store");
                let target_store = TicketStore::open_or_init(&target_workspace).expect("open target store");
                target_store.scan(true).expect("scan target store");
                let id: Uuid = perf.submodule_ticket_ids[0].parse().expect("fixture move id");
                let mut plan = store
                    .plan_move_preflight(&id, &target_workspace)
                    .expect("plan preflight");
                plan.blockers.retain(|blocker| {
                    !matches!(
                        blocker,
                        MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                            | MovePreflightBlocker::DirtyTrackedFiles { .. }
                    )
                });
                (perf, store, plan)
            },
            |(_perf, store, plan)| {
                let started = Instant::now();
                let outcome = store
                    .execute_move_with_journal(&plan)
                    .expect("execute move");
                let elapsed = started.elapsed();
                criterion::black_box(elapsed);
                assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);
                tracing::info!(
                    target: PERF_TRACE_TARGET,
                    benchmark = "move_execute_reference_heavy",
                    run_id = %Uuid::new_v4(),
                    fixture_profile = "git_ref_heavy_48_24_8x18",
                    workspace_scope = "ticket-submodule-a",
                    change_count = 0,
                    reindex_mode = "true",
                    p50_ms = elapsed.as_millis() as u64,
                    p95_ms = elapsed.as_millis() as u64,
                    p99_ms = elapsed.as_millis() as u64,
                    rewritten_files = outcome.journal.rewritten_path_files.len(),
                    elapsed_ms = elapsed.as_millis() as u64,
                    "ticket_api_benchmark_iteration"
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_move_rollback_reference_heavy(c: &mut Criterion) {
    c.bench_function("move_rollback_reference_heavy", |b| {
        b.iter_batched(
            || {
                let perf = materialize_git_fixture_with_ticket_perf_load(TicketPerfFixtureOptions::heavy())
                    .expect("perf fixture should materialize");
                let source_root = perf
                    .fixture
                    .store_root("ticket-submodule-a")
                    .expect("submodule store")
                    .to_path_buf();
                let target_workspace = perf.fixture.workspace_root.clone();
                let store = TicketStore::open_or_init(&source_root).expect("open source store");
                store.scan(true).expect("scan source store");
                let target_store = TicketStore::open_or_init(&target_workspace).expect("open target store");
                target_store.scan(true).expect("scan target store");
                let id: Uuid = perf.submodule_ticket_ids[0].parse().expect("fixture move id");
                let mut plan = store
                    .plan_move_preflight(&id, &target_workspace)
                    .expect("plan preflight");
                plan.blockers.retain(|blocker| {
                    !matches!(
                        blocker,
                        MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                            | MovePreflightBlocker::DirtyTrackedFiles { .. }
                    )
                });
                let outcome = store.execute_move_with_journal(&plan).expect("execute move");
                (perf, store, outcome.journal.id)
            },
            |(_perf, store, journal_id)| {
                let started = Instant::now();
                let outcome = store
                    .rollback_move_with_journal(journal_id)
                    .expect("rollback move");
                criterion::black_box(started.elapsed());
                assert!(outcome.rolled_back);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_open_or_init_root_perf_fixture(c: &mut Criterion) {
    init_perf_bench_tracing();
    c.bench_function("open_or_init_root_perf_fixture", |b| {
        b.iter_batched(
            || {
                let perf = materialize_fixture_with_ticket_perf_load(
                    TicketPerfFixtureOptions::heavy(),
                )
                .expect("perf fixture should materialize");
                let root_store = perf
                    .fixture
                    .store_root("ticket-root")
                    .expect("root store")
                    .to_path_buf();
                (perf, root_store)
            },
            |(_perf, root_store)| {
                let started = Instant::now();
                let (store, report) =
                    TicketStore::open_or_init_profiled(&root_store)
                        .expect("open store");
                let elapsed = started.elapsed();
                criterion::black_box(elapsed);
                let phase_count = report.phase_timings_ms.len();
                let (p50_ms, p95_ms, p99_ms) =
                    map_percentiles(&report.phase_timings_ms);
                criterion::black_box(report.phase_timings_ms);
                criterion::black_box(store);
                tracing::info!(
                    target: PERF_TRACE_TARGET,
                    benchmark = "open_or_init_root_perf_fixture",
                    run_id = %Uuid::new_v4(),
                    fixture_profile = "root_heavy",
                    workspace_scope = "ticket-root",
                    change_count = 0,
                    reindex_mode = "true",
                    p50_ms,
                    p95_ms,
                    p99_ms,
                    phase_count,
                    elapsed_ms = elapsed.as_millis() as u64,
                    "ticket_api_benchmark_iteration"
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_scan_reindex_root_perf_fixture(c: &mut Criterion) {
    init_perf_bench_tracing();
    c.bench_function("scan_reindex_root_perf_fixture", |b| {
        b.iter_batched(
            || {
                let perf = materialize_fixture_with_ticket_perf_load(
                    TicketPerfFixtureOptions::heavy(),
                )
                .expect("perf fixture should materialize");
                let root_store = perf
                    .fixture
                    .store_root("ticket-root")
                    .expect("root store")
                    .to_path_buf();
                let store =
                    TicketStore::open_or_init(&root_store).expect("open store");
                (perf, store)
            },
            |(_perf, store)| {
                let started = Instant::now();
                let report = store.scan(true).expect("scan(true)");
                let elapsed = started.elapsed();
                let (p50_ms, p95_ms, p99_ms) =
                    map_percentiles(&report.phase_timings_ms);
                criterion::black_box(started.elapsed());
                criterion::black_box(report.phase_timings_ms);
                tracing::info!(
                    target: PERF_TRACE_TARGET,
                    benchmark = "scan_reindex_root_perf_fixture",
                    run_id = %Uuid::new_v4(),
                    fixture_profile = "root_heavy",
                    workspace_scope = "ticket-root",
                    change_count = 0,
                    reindex_mode = "true",
                    p50_ms,
                    p95_ms,
                    p99_ms,
                    elapsed_ms = elapsed.as_millis() as u64,
                    "ticket_api_benchmark_iteration"
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_scan_incremental_root_perf_fixture(
    c: &mut Criterion,
    change_count: usize,
    label: &str,
) {
    init_perf_bench_tracing();
    c.bench_function(label, |b| {
        b.iter_batched(
            || {
                let perf = materialize_fixture_with_ticket_perf_load(
                    TicketPerfFixtureOptions::heavy(),
                )
                .expect("perf fixture should materialize");
                let root_store = perf
                    .fixture
                    .store_root("ticket-root")
                    .expect("root store")
                    .to_path_buf();
                let store =
                    TicketStore::open_or_init(&root_store).expect("open store");
                store.scan(true).expect("initial scan");
                append_incremental_fixture_tickets(
                    &root_store,
                    change_count,
                    change_count,
                );
                (perf, store)
            },
            |(_perf, store)| {
                let started = Instant::now();
                let report = store.scan(false).expect("scan(false)");
                let elapsed = started.elapsed();
                let (p50_ms, p95_ms, p99_ms) =
                    map_percentiles(&report.phase_timings_ms);
                criterion::black_box(started.elapsed());
                criterion::black_box(report.phase_timings_ms);
                criterion::black_box(report.root_entry_counts);
                tracing::info!(
                    target: PERF_TRACE_TARGET,
                    benchmark = label,
                    run_id = %Uuid::new_v4(),
                    fixture_profile = "root_heavy_incremental",
                    workspace_scope = "ticket-root",
                    change_count,
                    reindex_mode = "false",
                    p50_ms,
                    p95_ms,
                    p99_ms,
                    elapsed_ms = elapsed.as_millis() as u64,
                    "ticket_api_benchmark_iteration"
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_scan_incremental_root_perf_fixture_1(c: &mut Criterion) {
    bench_scan_incremental_root_perf_fixture(
        c,
        1,
        "scan_incremental_root_perf_fixture_1_change",
    );
}

fn bench_scan_incremental_root_perf_fixture_10(c: &mut Criterion) {
    bench_scan_incremental_root_perf_fixture(
        c,
        10,
        "scan_incremental_root_perf_fixture_10_changes",
    );
}

fn bench_scan_incremental_root_perf_fixture_100(c: &mut Criterion) {
    bench_scan_incremental_root_perf_fixture(
        c,
        100,
        "scan_incremental_root_perf_fixture_100_changes",
    );
}

fn bench_health_workflow_build_large_fixture(c: &mut Criterion) {
    init_perf_bench_tracing();
    let options = TicketPerfFixtureOptions::heavy();
    c.bench_function("health_workflow_build_large_fixture", |b| {
        b.iter_batched(
            || {
                let perf = materialize_fixture_with_ticket_perf_load(options)
                    .expect("perf fixture should materialize");
                let root_store = perf
                    .fixture
                    .store_root("ticket-root")
                    .expect("root store")
                    .to_path_buf();
                let store =
                    TicketStore::open_or_init(&root_store).expect("open store");
                store.scan(true).expect("scan store");
                let ids = parse_ids(&perf.root_ticket_ids);
                add_perf_edges(&store, &ids);
                let tickets =
                    store.list(None, None, None).expect("list tickets");
                let all_edges = store.list_all_edges().expect("list edges");
                (perf, store, tickets, all_edges)
            },
            |(_perf, store, tickets, all_edges)| {
                let started = Instant::now();
                let workflow = WorkflowModel::build(&store, tickets, all_edges)
                    .expect("build workflow");
                let elapsed = started.elapsed();
                criterion::black_box(elapsed);
                criterion::black_box(workflow);
                tracing::info!(
                    target: PERF_TRACE_TARGET,
                    benchmark = "health_workflow_build_large_fixture",
                    run_id = %Uuid::new_v4(),
                    fixture_profile = "root_heavy",
                    workspace_scope = "ticket-root",
                    change_count = 0,
                    reindex_mode = "true",
                    p50_ms = elapsed.as_millis() as u64,
                    p95_ms = elapsed.as_millis() as u64,
                    p99_ms = elapsed.as_millis() as u64,
                    elapsed_ms = elapsed.as_millis() as u64,
                    "ticket_api_benchmark_iteration"
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_health_collect_large_fixture(c: &mut Criterion) {
    init_perf_bench_tracing();
    let options = TicketPerfFixtureOptions::heavy();
    c.bench_function("health_collect_large_fixture", |b| {
        b.iter_batched(
            || {
                let perf = materialize_fixture_with_ticket_perf_load(options)
                    .expect("perf fixture should materialize");
                let root_store = perf
                    .fixture
                    .store_root("ticket-root")
                    .expect("root store")
                    .to_path_buf();
                let store =
                    TicketStore::open_or_init(&root_store).expect("open store");
                store.scan(true).expect("scan store");
                let ids = parse_ids(&perf.root_ticket_ids);
                add_perf_edges(&store, &ids);
                let tickets =
                    store.list(None, None, None).expect("list tickets");
                let all_edges = store.list_all_edges().expect("list edges");
                let workflow = WorkflowModel::build(
                    &store,
                    tickets.clone(),
                    all_edges.clone(),
                )
                .expect("build workflow");
                (perf, store, tickets, all_edges, workflow)
            },
            |(_perf, store, tickets, all_edges, workflow)| {
                let started = Instant::now();
                let report =
                    collect_findings(&store, &tickets, &all_edges, &workflow);
                let elapsed = started.elapsed();
                criterion::black_box(elapsed);
                criterion::black_box(report.findings.len());
                tracing::info!(
                    target: PERF_TRACE_TARGET,
                    benchmark = "health_collect_large_fixture",
                    run_id = %Uuid::new_v4(),
                    fixture_profile = "root_heavy",
                    workspace_scope = "ticket-root",
                    change_count = 0,
                    reindex_mode = "true",
                    p50_ms = elapsed.as_millis() as u64,
                    p95_ms = elapsed.as_millis() as u64,
                    p99_ms = elapsed.as_millis() as u64,
                    findings = report.findings.len(),
                    elapsed_ms = elapsed.as_millis() as u64,
                    "ticket_api_benchmark_iteration"
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_health_all_large_fixture(c: &mut Criterion) {
    init_perf_bench_tracing();
    let options = TicketPerfFixtureOptions {
        root_generated_ticket_count: 240,
        submodule_generated_ticket_count: 64,
        tracked_reference_file_count: 4,
        references_per_file: 10,
    };
    c.bench_function("health_all_large_fixture", |b| {
        b.iter_batched(
            || {
                let perf = materialize_fixture_with_ticket_perf_load(options)
                    .expect("perf fixture should materialize");
                let root_store = perf
                    .fixture
                    .store_root("ticket-root")
                    .expect("root store")
                    .to_path_buf();
                let store =
                    TicketStore::open_or_init(&root_store).expect("open store");
                store.scan(true).expect("scan store");
                let ids = parse_ids(&perf.root_ticket_ids);
                add_perf_edges(&store, &ids);
                (perf, store)
            },
            |(_perf, store)| {
                let started = Instant::now();
                let tickets =
                    store.list(None, None, None).expect("list tickets");
                let all_edges = store.list_all_edges().expect("list edges");
                let workflow = WorkflowModel::build(
                    &store,
                    tickets.clone(),
                    all_edges.clone(),
                )
                .expect("build workflow");
                let report =
                    collect_findings(&store, &tickets, &all_edges, &workflow);
                let elapsed = started.elapsed();
                criterion::black_box(report.findings.len());
                tracing::info!(
                    target: PERF_TRACE_TARGET,
                    benchmark = "health_all_large_fixture",
                    run_id = %Uuid::new_v4(),
                    fixture_profile = "root240_sub64_refs4x10",
                    workspace_scope = "ticket-root",
                    change_count = 0,
                    reindex_mode = "true",
                    p50_ms = elapsed.as_millis() as u64,
                    p95_ms = elapsed.as_millis() as u64,
                    p99_ms = elapsed.as_millis() as u64,
                    findings = report.findings.len(),
                    elapsed_ms = elapsed.as_millis() as u64,
                    "ticket_api_benchmark_iteration"
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });
    let mut group = c.benchmark_group("health_all_large_fixture_meta");
    group.throughput(Throughput::Elements(
        options.root_generated_ticket_count as u64,
    ));
    group.finish();
}

criterion_group!(
    benches,
    bench_open_or_init_root_perf_fixture,
    bench_scan_reindex_root_perf_fixture,
    bench_scan_incremental_root_perf_fixture_1,
    bench_scan_incremental_root_perf_fixture_10,
    bench_scan_incremental_root_perf_fixture_100,
    bench_move_preflight_reference_heavy,
    bench_move_execute_reference_heavy,
    bench_move_rollback_reference_heavy,
    bench_health_workflow_build_large_fixture,
    bench_health_collect_large_fixture,
    bench_health_all_large_fixture,
);
criterion_main!(benches);
