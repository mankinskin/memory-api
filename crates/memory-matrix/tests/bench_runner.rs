//! Focused test for the benchmark-matrix ingest + budget enforcement logic.
//!
//! Synthesizes Criterion `estimates.json` files (no real bench run) and asserts
//! that [`ingest_bench_results`] records `BenchmarkExecution`s, flags
//! over-budget cells, and reports cells without estimates as missing.

use std::fs;

use memory_matrix::{
    bench_id,
    bench_runner::ingest_bench_results,
};
use test_api::{
    BenchmarkQuery,
    TestStoreConfig,
};

fn write_estimates(
    criterion_root: &std::path::Path,
    id: &str,
    mean_ns: f64,
) {
    let dir = criterion_root.join(id).join("new");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("estimates.json"),
        format!(
            r#"{{
                "mean": {{ "point_estimate": {mean_ns} }},
                "median": {{ "point_estimate": {mean_ns} }},
                "std_dev": {{ "point_estimate": 1000.0 }}
            }}"#
        ),
    )
    .unwrap();
}

#[test]
fn ingest_flags_over_budget_and_records_benchmarks() {
    let tmp = tempfile::tempdir().unwrap();
    let criterion_root = tmp.path().join("criterion");
    let store_root = tmp.path().join(".test");
    let budgets_path = tmp.path().join("budgets.toml");
    fs::write(&budgets_path, "[budgets]\nget = 50\nscan = 3000\n").unwrap();

    // One clearly over-budget get (75 ms > 50 ms) and one within-budget scan.
    write_estimates(&criterion_root, &bench_id("ticket", "get"), 75_000_000.0);
    write_estimates(&criterion_root, &bench_id("ticket", "scan"), 10_000_000.0);

    let store = TestStoreConfig::new(&store_root, "default");
    let report = ingest_bench_results(
        &criterion_root,
        &budgets_path,
        &store,
        "03ed4121",
    )
    .expect("ingest should succeed");

    // Two cells had estimates; the rest of the matrix is reported missing.
    assert_eq!(report.results.len(), 2);
    assert!(
        !report.missing.is_empty(),
        "uncovered cells should be missing"
    );

    let over = report.over_budget();
    assert_eq!(over.len(), 1, "exactly the get cell is over budget");
    assert_eq!(over[0].domain, "ticket");
    assert_eq!(over[0].operation, "get");

    // Persisted into the test store with budget + over_budget metadata.
    let persisted = store
        .list_benchmarks(&BenchmarkQuery::default())
        .expect("benchmarks should be queryable");
    assert_eq!(persisted.len(), 2);

    let over_persisted = store
        .list_benchmarks(&BenchmarkQuery {
            over_budget: Some(true),
            ..Default::default()
        })
        .expect("over-budget query should work");
    assert_eq!(over_persisted.len(), 1);
    assert_eq!(over_persisted[0].operation, "get");
    assert_eq!(over_persisted[0].budget_ns, Some(50_000_000));
}
