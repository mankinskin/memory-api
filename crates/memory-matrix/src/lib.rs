//! Data-driven cross-domain operation test matrix.
//!
//! Exercises the basic operations of every memory domain against a freshly
//! materialized [`memory_fixtures`] workspace and records each cell as a
//! `test-api` [`test_api::ValidationExecution`] with a wall-clock duration.

mod domains;
mod matrix;

pub mod bench_runner;

pub use matrix::{
    Cell,
    CellRecord,
    CellResult,
    CellSpec,
    ExpectedOutcome,
    FIXTURE_PROFILE_DEFAULT,
    MatrixCtx,
    MatrixRun,
    OPERATIONS,
    TRANSPORTS,
    bench_id,
    cells,
    run_matrix,
    run_one,
    run_ticket_get_mcp_subprocess_failure_probe,
    transport_cells,
};

pub use memory_fixtures::{
    FixtureError as FixtureLoadError,
    LoadedFixture as Fixture,
    materialize_fixture as materialize,
};
