use std::time::Instant;

use chrono::Utc;
use criterion::{
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use memory_fixtures::{
    TicketPerfFixtureOptions,
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

fn bench_move_preflight_reference_heavy(c: &mut Criterion) {
    c.bench_function("move_preflight_reference_heavy", |b| {
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
                let id: Uuid = perf.submodule_ticket_ids[0].parse().expect("fixture move id");
                (perf, store, target_workspace, id)
            },
            |(_perf, store, target_workspace, id)| {
                let started = Instant::now();
                let plan = store
                    .plan_move_preflight(&id, &target_workspace)
                    .expect("plan preflight");
                criterion::black_box(started.elapsed());
                criterion::black_box(plan.path_reference_files.len());
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_move_execute_reference_heavy(c: &mut Criterion) {
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
                criterion::black_box(started.elapsed());
                assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_health_all_large_fixture(c: &mut Criterion) {
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
                let store = TicketStore::open_or_init(&root_store).expect("open store");
                store.scan(true).expect("scan store");
                let ids = parse_ids(&perf.root_ticket_ids);
                add_perf_edges(&store, &ids);
                (perf, store)
            },
            |(_perf, store)| {
                let tickets = store.list(None, None, None).expect("list tickets");
                let all_edges = store.list_all_edges().expect("list edges");
                let workflow = WorkflowModel::build(&store, tickets.clone(), all_edges.clone())
                    .expect("build workflow");
                let report = collect_findings(&store, &tickets, &all_edges, &workflow);
                criterion::black_box(report.findings.len());
            },
            criterion::BatchSize::SmallInput,
        );
    });
    let mut group = c.benchmark_group("health_all_large_fixture_meta");
    group.throughput(Throughput::Elements(options.root_generated_ticket_count as u64));
    group.finish();
}

criterion_group!(
    benches,
    bench_move_preflight_reference_heavy,
    bench_move_execute_reference_heavy,
    bench_health_all_large_fixture,
);
criterion_main!(benches);