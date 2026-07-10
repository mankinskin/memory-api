use std::path::Path;

use chrono::Utc;
use test_api::{
    BudgetTable,
    TestStoreConfig,
    ValidationLinks,
    ingest_criterion_estimates,
};

use crate::{
    bench_id,
    cells,
};

/// The ingested result for one benchmark cell.
#[derive(Debug, Clone)]
pub struct BenchCellResult {
    pub domain: String,
    pub operation: String,
    pub mean_ns: u64,
    pub budget_ns: Option<u64>,
    pub over_budget: bool,
}

/// Summary of a benchmark-matrix ingest pass.
#[derive(Debug, Default)]
pub struct BenchReport {
    pub results: Vec<BenchCellResult>,
    pub missing: Vec<String>,
}

impl BenchReport {
    /// Cells whose mean latency exceeded the configured budget.
    pub fn over_budget(&self) -> Vec<&BenchCellResult> {
        self.results
            .iter()
            .filter(|cell| cell.over_budget)
            .collect()
    }
}

/// Read every cell's Criterion `estimates.json` under `criterion_root`,
/// apply the budget table at `budgets_path`, record each as a
/// `BenchmarkExecution` in `store`, and return a [`BenchReport`].
///
/// Cells without an `estimates.json` (e.g. a bench run never produced them)
/// are reported under [`BenchReport::missing`] rather than silently dropped.
pub fn ingest_bench_results(
    criterion_root: &Path,
    budgets_path: &Path,
    store: &TestStoreConfig,
    ticket_id: &str,
) -> Result<BenchReport, String> {
    let table =
        BudgetTable::load(budgets_path).map_err(|err| err.to_string())?;
    let now = Utc::now();
    let run_id = format!("bench-matrix-{}", now.to_rfc3339());
    let mut report = BenchReport::default();

    for (domain, operation) in cells() {
        let id = bench_id(domain, operation);
        let estimates =
            criterion_root.join(&id).join("new").join("estimates.json");
        if !estimates.is_file() {
            report.missing.push(id);
            continue;
        }

        let mut execution = ingest_criterion_estimates(
            &estimates,
            format!("exec-bench-{id}"),
            &id,
            operation,
            domain,
            now,
        )
        .map_err(|err| err.to_string())?;
        execution.run_id = Some(run_id.clone());
        execution.apply_budget(table.budget_ns(domain, operation));
        execution.links = ValidationLinks {
            ticket_ids: vec![ticket_id.to_string()],
            ..Default::default()
        };
        store
            .record_benchmark(&execution)
            .map_err(|err| err.to_string())?;

        report.results.push(BenchCellResult {
            domain: domain.to_string(),
            operation: operation.to_string(),
            mean_ns: execution.mean_ns,
            budget_ns: execution.budget_ns,
            over_budget: execution.over_budget,
        });
    }

    Ok(report)
}
