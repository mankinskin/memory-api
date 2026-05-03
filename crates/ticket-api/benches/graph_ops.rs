//! Criterion benchmarks for the three-phase BFS graph query pipeline.
//!
//! These replace the ad-hoc Python scripts (`bench.py`, `bench2.py`, `bench3.py`,
//! `time_one.py`, `check_stats.py`) and `stress_graph.py` with reproducible,
//! statistically rigorous measurements that don't depend on a running HTTP server.
//!
//! ## Phases under test
//!
//! | Phase | What               | Benchmark name                     |
//! |-------|--------------------|------------------------------------|
//! | 1     | Load all edges     | `phase1_list_all_edges`            |
//! | 2     | BFS in-memory      | `phase2_bfs_in_memory`             |
//! | 3a    | Batch meta fetch   | `phase3_get_indexed_many`          |
//! | 3b    | Per-node meta fetch| `phase3_get_indexed_one_by_one`    |
//! | All   | Full pipeline      | `pipeline_full`                    |
//! | Conc  | N concurrent runs  | `pipeline_concurrent/{2,4,8,16,32}`|
//!
//! ## Fixture
//!
//! 360 tickets + ~630 edges, matching production workspace size.
//! The BFS tree from root visits exactly 39 nodes at depth ≤ 4.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, OnceLock},
    thread,
};
use tempfile::TempDir;
use ticket_api::{
    model::{edge::EdgeRecord, filesystem::ScanRoot},
    storage::store::TicketStore,
};
use uuid::Uuid;

// (neighbor, edge_from, edge_to, edge_kind) — mirrors graph.rs
type AdjEntry = (Uuid, Uuid, Uuid, String);

// ── Fixture ───────────────────────────────────────────────────────────────────

struct Fixture {
    _dir: TempDir,
    store: Arc<TicketStore>,
    root: Uuid,
    /// Node IDs visited by BFS from root at depth ≤ 4 (~39 nodes).
    bfs_node_ids: Vec<Uuid>,
}

// Safety: TempDir, TicketStore, Uuid, and Vec<Uuid> are all Send + Sync.
unsafe impl Sync for Fixture {}

/// Build a realistic test fixture: 360 tickets and ~630 edges.
///
/// Topology:
/// - A tree of 39 nodes reachable from `root` at depth ≤ 4 (5→10→10→13 fan-out)
/// - 321 background tickets connected by ~592 "linked" edges (prime-step pattern
///   to avoid accidental duplicates)
///
/// Note: uses "linked" edges (not "depends_on") because "linked" has
/// `acyclic_enforced = false`, so setup avoids O(N²) cycle checks.
fn build_fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir for bench fixture");
    let store = TicketStore::open(dir.path()).expect("open store");
    store
        .add_scan_root(ScanRoot {
            path: dir.path().join("tickets"),
            label: "bench".into(),
        })
        .expect("add scan root");

    // ── 360 tickets ──────────────────────────────────────────────────────────
    let ids: Vec<Uuid> = (0..360usize)
        .map(|i| {
            store
                .create(
                    None,
                    "tracker-improvement",
                    Some(&format!("Bench ticket {i}")),
                    Some("new"),
                    Default::default(),
                    None,
                    None,
                )
                .expect("create ticket")
        })
        .collect();

    let root = ids[0];
    let now = chrono::Utc::now();
    let linked = |from: Uuid, to: Uuid| EdgeRecord {
        from,
        to,
        kind: "linked".into(),
        created_at: now,
    };

    // ── Reachable subgraph from root (39 nodes, depth ≤ 4) ──────────────────
    //
    //   depth 0: ids[0]               → 1 node
    //   depth 1: ids[1..=5]           → 5 nodes  (5 edges)
    //   depth 2: ids[6..=15]          → 10 nodes (10 edges, 2 per depth-1)
    //   depth 3: ids[16..=25]         → 10 nodes (10 edges, 1 per depth-2)
    //   depth 4: ids[26..=38]         → 13 nodes (13 edges, some depth-3 get 2)
    //   ─────────────────────────────────────────
    //   total:                         39 nodes  38 edges

    // depth 1
    for i in 1..=5usize {
        store.add_edge(linked(root, ids[i])).expect("add d1 edge");
    }
    // depth 2: 2 children per depth-1
    for i in 0..5usize {
        store.add_edge(linked(ids[1 + i], ids[6 + i * 2])).expect("add d2a edge");
        store.add_edge(linked(ids[1 + i], ids[6 + i * 2 + 1])).expect("add d2b edge");
    }
    // depth 3: 1 child per depth-2
    for i in 0..10usize {
        store.add_edge(linked(ids[6 + i], ids[16 + i])).expect("add d3 edge");
    }
    // depth 4: first 8 depth-3 nodes get 1 child; last 2 get 0; pad to 13
    for i in 0..8usize {
        store.add_edge(linked(ids[16 + i], ids[26 + i])).expect("add d4a edge");
    }
    // 5 more to reach 13 depth-4 nodes (nodes 34-38 as second children of d3[0..4])
    for i in 0..5usize {
        store.add_edge(linked(ids[16 + i], ids[34 + i])).expect("add d4b edge");
    }

    let bfs_node_ids: Vec<Uuid> = (0..39).map(|i| ids[i]).collect();
    let tree_edges = 38usize; // 5+10+10+8+5

    // ── Background edges: prime-step pattern to fill to ~630 total ───────────
    let bg_start = 39usize; // first index not in the BFS tree
    let bg_len = 360 - bg_start; // 321 background nodes
    let step = 37usize; // prime ensures wide, non-repeating distribution
    let target = 630usize;

    for seq in 0..(target - tree_edges) {
        let from_idx = bg_start + seq % bg_len;
        let to_idx = bg_start + (seq * step + 1) % bg_len;
        if from_idx != to_idx {
            // Ignore duplicate-key errors; exact edge count is approximate.
            let _ = store.add_edge(linked(ids[from_idx], ids[to_idx]));
        }
    }

    Fixture {
        _dir: dir,
        store: Arc::new(store),
        root,
        bfs_node_ids,
    }
}

/// Global fixture — built once, reused across all benchmark groups.
static FIXTURE: OnceLock<Fixture> = OnceLock::new();

fn fixture() -> &'static Fixture {
    FIXTURE.get_or_init(build_fixture)
}

// ── BFS helpers (mirrors graph.rs) ────────────────────────────────────────────

fn build_adjacency(edges: &[EdgeRecord]) -> HashMap<Uuid, Vec<AdjEntry>> {
    let mut adj: HashMap<Uuid, Vec<AdjEntry>> = HashMap::with_capacity(edges.len() * 2);
    for e in edges {
        adj.entry(e.from)
            .or_default()
            .push((e.to, e.from, e.to, e.kind.clone()));
        adj.entry(e.to)
            .or_default()
            .push((e.from, e.from, e.to, e.kind.clone()));
    }
    adj
}

fn run_bfs(adj: &HashMap<Uuid, Vec<AdjEntry>>, root: Uuid, depth_limit: usize) -> HashMap<Uuid, usize> {
    let mut visited: HashMap<Uuid, usize> = HashMap::new();
    let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
    queue.push_back((root, 0));
    while let Some((node, depth)) = queue.pop_front() {
        if visited.contains_key(&node) {
            continue;
        }
        visited.insert(node, depth);
        if depth >= depth_limit {
            continue;
        }
        if let Some(neighbors) = adj.get(&node) {
            for (neighbor, ..) in neighbors {
                if !visited.contains_key(neighbor) {
                    queue.push_back((*neighbor, depth + 1));
                }
            }
        }
    }
    visited
}

// ── Phase 1: load all edges ───────────────────────────────────────────────────

fn bench_phase1_list_edges(c: &mut Criterion) {
    let fx = fixture();
    c.bench_function("phase1_list_all_edges", |b| {
        b.iter(|| criterion::black_box(fx.store.list_all_edges().unwrap()))
    });
}

// ── Phase 2: BFS over pre-built adjacency map ─────────────────────────────────

fn bench_phase2_bfs(c: &mut Criterion) {
    let fx = fixture();
    let edges = fx.store.list_all_edges().unwrap();
    let adj = build_adjacency(&edges);
    c.bench_function("phase2_bfs_in_memory", |b| {
        b.iter(|| criterion::black_box(run_bfs(&adj, fx.root, 4)))
    });
}

// ── Phase 3a: batch metadata fetch (single ReDB transaction) ─────────────────

fn bench_phase3_batch(c: &mut Criterion) {
    let fx = fixture();
    let ids = &fx.bfs_node_ids;
    c.bench_function("phase3_get_indexed_many", |b| {
        b.iter(|| criterion::black_box(fx.store.get_indexed_many(ids).unwrap()))
    });
}

// ── Phase 3b: per-node metadata fetch (N separate transactions) ───────────────
// Baseline for the old approach that get_indexed_many replaced.

fn bench_phase3_individual(c: &mut Criterion) {
    let fx = fixture();
    let ids = &fx.bfs_node_ids;
    c.bench_function("phase3_get_indexed_one_by_one", |b| {
        b.iter(|| {
            for id in ids {
                criterion::black_box(fx.store.get_indexed(id).unwrap());
            }
        })
    });
}

// ── Full pipeline: phases 1 + 2 + 3 end-to-end ───────────────────────────────

fn bench_pipeline_full(c: &mut Criterion) {
    let fx = fixture();
    c.bench_function("pipeline_full", |b| {
        b.iter(|| {
            let edges = fx.store.list_all_edges().unwrap();
            let adj = build_adjacency(&edges);
            let visited = run_bfs(&adj, fx.root, 4);
            let node_ids: Vec<Uuid> = visited.keys().copied().collect();
            criterion::black_box(fx.store.get_indexed_many(&node_ids).unwrap())
        })
    });
}

// ── Concurrent pipeline: N threads, barrier-synchronized start ───────────────
// Replaces stress_graph.py. Shows scaling under concurrent load at the
// storage layer (no HTTP overhead).

fn bench_pipeline_concurrent(c: &mut Criterion) {
    let fx = fixture();
    let store = Arc::clone(&fx.store);
    let root = fx.root;

    let mut group = c.benchmark_group("pipeline_concurrent");
    for &conc in &[2usize, 4, 8, 16, 32] {
        group.throughput(Throughput::Elements(conc as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(conc),
            &conc,
            |b, &conc| {
                b.iter(|| {
                    let barrier = Arc::new(std::sync::Barrier::new(conc));
                    let handles: Vec<_> = (0..conc)
                        .map(|_| {
                            let store = Arc::clone(&store);
                            let bar = Arc::clone(&barrier);
                            thread::spawn(move || {
                                bar.wait(); // synchronized start
                                let edges = store.list_all_edges().unwrap();
                                let adj = build_adjacency(&edges);
                                let visited = run_bfs(&adj, root, 4);
                                let ids: Vec<Uuid> = visited.keys().copied().collect();
                                criterion::black_box(store.get_indexed_many(&ids).unwrap())
                            })
                        })
                        .collect();
                    for h in handles {
                        h.join().unwrap();
                    }
                })
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_phase1_list_edges,
    bench_phase2_bfs,
    bench_phase3_batch,
    bench_phase3_individual,
    bench_pipeline_full,
    bench_pipeline_concurrent,
);
criterion_main!(benches);
