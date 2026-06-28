//! Criterion benchmark for the cross-domain operation matrix.
//!
//! Benchmarks the same `domain x operation` cells as the test matrix. Each
//! cell is measured against a **fresh** materialized fixture per iteration
//! (`iter_batched` setup is untimed), so mutating operations stay isolated and
//! the measured routine reflects one end-to-end operation against the fixture.
//!
//! The ingest runner (`bench-matrix` binary) reads each cell's
//! `target/criterion/<bench_id>/new/estimates.json`, compares the mean against
//! the per-operation budget, and records a `test-api` `BenchmarkExecution`.

use std::time::Duration;

use criterion::{
    BatchSize,
    Criterion,
    criterion_group,
    criterion_main,
};

use memory_matrix::{
    MatrixCtx,
    bench_id,
    cells,
    materialize,
    run_one,
};

fn bench_operation_matrix(c: &mut Criterion) {
    for (domain, operation) in cells() {
        let id = bench_id(domain, operation);
        c.bench_function(&id, |b| {
            b.iter_batched(
                || {
                    let fixture =
                        materialize().expect("fixture should materialize");
                    let ctx = MatrixCtx::new(fixture.workspace_root.clone());
                    (fixture, ctx)
                },
                |(fixture, ctx)| {
                    // Outcome is irrelevant to the measurement; blocked cells
                    // simply return fast.
                    let _ = run_one(domain, operation, &ctx);
                    drop(fixture);
                },
                BatchSize::SmallInput,
            );
        });
    }
}

fn configured() -> Criterion {
    // Bounded sampling: this is a budget tripwire, not a precision micro-bench,
    // and each iteration re-materializes the fixture.
    Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_millis(50))
        .measurement_time(Duration::from_millis(200))
}

criterion_group! {
    name = benches;
    config = configured();
    targets = bench_operation_matrix
}
criterion_main!(benches);
