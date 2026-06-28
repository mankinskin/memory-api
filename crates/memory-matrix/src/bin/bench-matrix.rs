//! `bench-matrix` — single-command runner for the cross-domain benchmark matrix.
//!
//! Runs the Criterion bench (`benches/operation_matrix.rs`), ingests each
//! cell's estimates into `test-api` as a `BenchmarkExecution` with its budget
//! and `over_budget` flag, prints a summary, and exits non-zero when any
//! operation exceeds its latency budget.
//!
//! Usage:
//!   cargo run -p memory-matrix --bin bench-matrix [-- FLAGS]
//!
//! Flags:
//!   --skip-bench              Ingest existing Criterion results without re-running the bench.
//!   --store-root <path>       Test (`.test`) store root. Default: memory-api/.test
//!   --criterion-root <path>   Criterion output root. Default: $CARGO_TARGET_DIR/criterion or target/criterion
//!   --budgets <path>          Budget table TOML. Default: this crate's budgets.toml

use std::path::PathBuf;
use std::process::{exit, Command};

use memory_matrix::bench_runner::ingest_bench_results;
use test_api::TestStoreConfig;

const TICKET_ID: &str = "03ed4121";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let skip_bench = args.iter().any(|arg| arg == "--skip-bench");

    let store_root = flag(&args, "--store-root")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("memory-api/.test"));
    let criterion_root = flag(&args, "--criterion-root")
        .map(PathBuf::from)
        .unwrap_or_else(default_criterion_root);
    let budgets_path = flag(&args, "--budgets").map(PathBuf::from).unwrap_or_else(
        || PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/budgets.toml")),
    );

    if !skip_bench {
        eprintln!("running: cargo bench -p memory-matrix --bench operation_matrix");
        let status = Command::new("cargo")
            .args(["bench", "-p", "memory-matrix", "--bench", "operation_matrix"])
            .status()
            .unwrap_or_else(|err| {
                eprintln!("failed to launch cargo bench: {err}");
                exit(2);
            });
        if !status.success() {
            eprintln!("cargo bench failed");
            exit(2);
        }
    }

    let store = TestStoreConfig::new(store_root, "default");
    let report = match ingest_bench_results(
        &criterion_root,
        &budgets_path,
        &store,
        TICKET_ID,
    ) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("ingest failed: {err}");
            exit(2);
        },
    };

    println!(
        "benchmark matrix: {} cells ingested, {} missing",
        report.results.len(),
        report.missing.len()
    );
    for cell in &report.results {
        let budget = cell
            .budget_ns
            .map(|ns| format!("{} ms", ns / 1_000_000))
            .unwrap_or_else(|| "-".to_string());
        let status = if cell.over_budget { "OVER" } else { "ok" };
        println!(
            "  {:<8} {:<8} mean={:>9.3} ms  budget={:<9} {}",
            cell.domain,
            cell.operation,
            cell.mean_ns as f64 / 1_000_000.0,
            budget,
            status
        );
    }
    for missing in &report.missing {
        println!("  MISSING estimates for {missing}");
    }

    let over_budget = report.over_budget();
    if !over_budget.is_empty() {
        eprintln!("{} operation(s) exceeded their budget", over_budget.len());
        exit(1);
    }
    if !report.missing.is_empty() {
        eprintln!(
            "{} cell(s) missing Criterion estimates (run without --skip-bench)",
            report.missing.len()
        );
        exit(3);
    }
    println!("all operations within budget");
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn default_criterion_root() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"))
        .join("criterion")
}
