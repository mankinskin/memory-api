//! Benchmark result model, Criterion ingest, and latency budget table.
//!
//! Complements [`crate::ValidationExecution`] (pass/fail evidence) with
//! quantitative latency measurements that can be compared against
//! per-operation maximum-latency budgets.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    TestError,
    ValidationLinks,
};

/// A single benchmark measurement for one operation, optionally compared
/// against a latency budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkExecution {
    pub id: String,
    pub benchmark_name: String,
    /// Logical operation, e.g. `ticket.get`.
    pub operation: String,
    /// Domain the operation belongs to, e.g. `ticket`.
    pub domain: String,
    pub executed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub mean_ns: u64,
    pub median_ns: u64,
    pub std_dev_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_ns: Option<u64>,
    #[serde(default)]
    pub over_budget: bool,
    #[serde(default)]
    pub links: ValidationLinks,
}

impl BenchmarkExecution {
    pub fn new(
        id: impl Into<String>,
        benchmark_name: impl Into<String>,
        operation: impl Into<String>,
        domain: impl Into<String>,
        executed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: id.into(),
            benchmark_name: benchmark_name.into(),
            operation: operation.into(),
            domain: domain.into(),
            executed_at,
            run_id: None,
            mean_ns: 0,
            median_ns: 0,
            std_dev_ns: 0,
            min_ns: 0,
            max_ns: 0,
            throughput: None,
            budget_ns: None,
            over_budget: false,
            links: ValidationLinks::default(),
        }
    }

    /// Stamp the budget for this benchmark and recompute `over_budget`.
    ///
    /// A benchmark is over budget when its `mean_ns` exceeds `budget_ns`.
    pub fn apply_budget(
        &mut self,
        budget_ns: Option<u64>,
    ) {
        self.budget_ns = budget_ns;
        self.over_budget = match budget_ns {
            Some(budget) => self.mean_ns > budget,
            None => false,
        };
    }

    pub fn interoperability_gaps(&self) -> Vec<&'static str> {
        let mut gaps = Vec::new();
        if self.domain.trim().is_empty() {
            gaps.push("missing domain");
        }
        if self.operation.trim().is_empty() {
            gaps.push("missing operation");
        }
        if self.run_id.as_deref().is_none() {
            gaps.push("missing run_id");
        }
        if !self.links.has_traceability_links() {
            gaps.push("missing spec, acceptance, or ticket links");
        }
        gaps
    }

    pub fn validate_interoperability_contract(&self) -> Result<(), TestError> {
        let gaps = self.interoperability_gaps();
        if gaps.is_empty() {
            return Ok(());
        }

        Err(TestError::InteroperabilityContract {
            record_kind: "benchmark-execution".to_string(),
            detail: gaps.join(", "),
        })
    }
}

/// Filter for querying stored benchmark executions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BenchmarkQuery {
    pub domain: Option<String>,
    pub operation: Option<String>,
    /// When set, only return benchmarks whose `over_budget` matches.
    pub over_budget: Option<bool>,
    pub limit: Option<usize>,
}

/// A table mapping `domain.operation` keys to maximum-latency budgets.
///
/// Budgets are authored in milliseconds (human-friendly) and exposed in
/// nanoseconds (matching the benchmark measurement unit).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BudgetTable {
    #[serde(default)]
    pub budgets: BTreeMap<String, u64>,
}

impl BudgetTable {
    /// Load a budget table from a TOML file. Returns an empty table when the
    /// file does not exist so ingest can run without a budget config.
    pub fn load(path: &Path) -> Result<Self, TestError> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|err| TestError::BudgetParse {
                path: path.to_path_buf(),
                detail: err.to_string(),
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(TestError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Budget in nanoseconds for a `domain.operation`, if configured.
    pub fn budget_ns(
        &self,
        domain: &str,
        operation: &str,
    ) -> Option<u64> {
        let key = format!("{domain}.{operation}");
        self.budgets
            .get(&key)
            .or_else(|| self.budgets.get(operation))
            .map(|ms| ms.saturating_mul(1_000_000))
    }
}

// ── Criterion ingest ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CriterionPointEstimate {
    point_estimate: f64,
    #[serde(default)]
    confidence_interval: Option<CriterionConfidenceInterval>,
}

#[derive(Debug, Deserialize)]
struct CriterionConfidenceInterval {
    lower_bound: f64,
    upper_bound: f64,
}

#[derive(Debug, Deserialize)]
struct CriterionEstimates {
    mean: CriterionPointEstimate,
    median: CriterionPointEstimate,
    std_dev: CriterionPointEstimate,
}

fn round_ns(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        value.round() as u64
    } else {
        0
    }
}

/// Ingest a Criterion `estimates.json` file into a [`BenchmarkExecution`].
///
/// `mean_ns`/`median_ns`/`std_dev_ns` map from the corresponding point
/// estimates. `min_ns`/`max_ns` map from the mean's confidence-interval bounds
/// (Criterion's `estimates.json` does not record raw sample extremes).
pub fn ingest_criterion_estimates(
    estimates_path: &Path,
    id: impl Into<String>,
    benchmark_name: impl Into<String>,
    operation: impl Into<String>,
    domain: impl Into<String>,
    executed_at: DateTime<Utc>,
) -> Result<BenchmarkExecution, TestError> {
    let text =
        std::fs::read_to_string(estimates_path).map_err(|source| TestError::Io {
            path: estimates_path.to_path_buf(),
            source,
        })?;
    let estimates: CriterionEstimates =
        serde_json::from_str(&text).map_err(|err| TestError::CriterionIngest {
            path: estimates_path.to_path_buf(),
            detail: err.to_string(),
        })?;

    let mut execution =
        BenchmarkExecution::new(id, benchmark_name, operation, domain, executed_at);
    execution.mean_ns = round_ns(estimates.mean.point_estimate);
    execution.median_ns = round_ns(estimates.median.point_estimate);
    execution.std_dev_ns = round_ns(estimates.std_dev.point_estimate);

    let (min_ns, max_ns) = match &estimates.mean.confidence_interval {
        Some(ci) => (round_ns(ci.lower_bound), round_ns(ci.upper_bound)),
        None => (execution.mean_ns, execution.mean_ns),
    };
    execution.min_ns = min_ns;
    execution.max_ns = max_ns;

    Ok(execution)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 28, 12, 0, 0).single().unwrap()
    }

    #[test]
    fn apply_budget_flags_over_budget_on_mean() {
        let mut exec = BenchmarkExecution::new("b1", "fixture_scan", "scan", "ticket", at());
        exec.run_id = Some("run-1".to_string());
        exec.links.ticket_ids = vec!["ticket-1".to_string()];
        exec.mean_ns = 120_000_000;

        exec.apply_budget(Some(100_000_000));
        assert!(exec.over_budget);
        assert_eq!(exec.budget_ns, Some(100_000_000));

        exec.apply_budget(Some(150_000_000));
        assert!(!exec.over_budget);

        exec.apply_budget(None);
        assert!(!exec.over_budget);
        assert_eq!(exec.budget_ns, None);
    }

    #[test]
    fn budget_table_resolves_domain_operation_then_operation() {
        let mut budgets = BTreeMap::new();
        budgets.insert("ticket.get".to_string(), 50u64);
        budgets.insert("search".to_string(), 200u64);
        let table = BudgetTable { budgets };

        assert_eq!(table.budget_ns("ticket", "get"), Some(50_000_000));
        // falls back to bare operation key
        assert_eq!(table.budget_ns("spec", "search"), Some(200_000_000));
        assert_eq!(table.budget_ns("ticket", "delete"), None);
    }

    #[test]
    fn budget_table_load_missing_file_is_empty() {
        let dir = TempDir::new().unwrap();
        let table = BudgetTable::load(&dir.path().join("budgets.toml")).unwrap();
        assert!(table.budgets.is_empty());
    }

    #[test]
    fn interoperability_contract_requires_run_grouping_and_traceability() {
        let mut exec = BenchmarkExecution::new("b1", "fixture_scan", "scan", "ticket", at());
        let gaps = exec.interoperability_gaps();
        assert!(gaps.contains(&"missing run_id"));
        assert!(gaps.contains(&"missing spec, acceptance, or ticket links"));

        exec.run_id = Some("run-1".to_string());
        exec.links.ticket_ids = vec!["ticket-1".to_string()];
        assert!(exec.validate_interoperability_contract().is_ok());
    }

    #[test]
    fn ingest_maps_estimates_json_to_benchmark_execution() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("estimates.json");
        std::fs::write(
            &path,
            r#"{
                "mean": {
                    "confidence_interval": { "lower_bound": 900.0, "upper_bound": 1100.0 },
                    "point_estimate": 1000.0,
                    "standard_error": 25.0
                },
                "median": { "point_estimate": 980.0 },
                "median_abs_dev": { "point_estimate": 30.0 },
                "slope": null,
                "std_dev": { "point_estimate": 50.0 }
            }"#,
        )
        .unwrap();

        let exec = ingest_criterion_estimates(
            &path,
            "exec-bench-1",
            "fixture_scan_reindex_root_store",
            "scan",
            "ticket",
            at(),
        )
        .unwrap();

        assert_eq!(exec.mean_ns, 1000);
        assert_eq!(exec.median_ns, 980);
        assert_eq!(exec.std_dev_ns, 50);
        assert_eq!(exec.min_ns, 900);
        assert_eq!(exec.max_ns, 1100);
    }

    #[test]
    fn ingest_then_budget_marks_over_budget() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("estimates.json");
        std::fs::write(
            &path,
            r#"{
                "mean": { "point_estimate": 75000000.0 },
                "median": { "point_estimate": 74000000.0 },
                "std_dev": { "point_estimate": 1000000.0 }
            }"#,
        )
        .unwrap();

        let mut exec = ingest_criterion_estimates(
            &path, "exec-bench-2", "get_by_id", "get", "ticket", at(),
        )
        .unwrap();

        let table = BudgetTable {
            budgets: BTreeMap::from([("ticket.get".to_string(), 50u64)]),
        };
        exec.apply_budget(table.budget_ns("ticket", "get"));

        assert_eq!(exec.budget_ns, Some(50_000_000));
        assert!(exec.over_budget, "75ms mean should exceed 50ms budget");
    }
}
