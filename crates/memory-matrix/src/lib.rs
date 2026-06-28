//! Data-driven cross-domain operation test matrix.
//!
//! Exercises the basic operations of every memory domain against a freshly
//! materialized [`memory_fixtures`] workspace and records each cell as a
//! `test-api` [`test_api::ValidationExecution`] with a wall-clock duration.

mod domains;
mod matrix;

pub mod bench_runner;

pub use matrix::{
    bench_id, cells, run_matrix, run_one, Cell, CellRecord, CellResult,
    MatrixCtx, MatrixRun, OPERATIONS,
};

pub use memory_fixtures::{
    materialize_fixture as materialize, FixtureError as FixtureLoadError,
    LoadedFixture as Fixture,
};
