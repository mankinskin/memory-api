//! Test-store index generator (ticket `90de77b1`).
//!
//! Reads recorded validation executions and benchmark executions and renders a
//! committed summary index — per `domain.operation` timings, dedicated issue
//! and slow sections, and run metadata — following the domain-owned thin
//! generator architecture used by the other store indexes.
//!
//! Generation is deterministic: identical input yields an identical sidecar,
//! markdown, and digest, so the artifacts are safe to commit and diff in hooks.

use std::collections::BTreeMap;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    BenchmarkExecution,
    ValidationExecution,
    ValidationOutcome,
    ValidationSpec,
};

/// Provenance comment written at the top of the generated markdown.
pub const TEST_INDEX_FILE_COMMENT: &str =
    "<!-- test-index:file generated=true -->";

/// Inputs for a single store-index generation pass.
pub struct TestStoreIndexInput<'a> {
    pub executions: &'a [ValidationExecution],
    pub specs: &'a [ValidationSpec],
    pub benchmarks: &'a [BenchmarkExecution],
}

/// Aggregated per-validation-spec execution statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationGroupSummary {
    pub validation_spec_id: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub blocked: usize,
    pub latest_outcome: String,
    pub source_path: Option<String>,
    pub test_id: Option<String>,
    pub domain: Option<String>,
    pub operation: Option<String>,
    pub transport: Option<String>,
    pub run_id: Option<String>,
    pub last_duration_ms: Option<u64>,
    pub min_duration_ms: Option<u64>,
    pub median_duration_ms: Option<u64>,
    pub max_duration_ms: Option<u64>,
}

/// Aggregated per-`domain.operation` benchmark statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkGroupSummary {
    pub key: String,
    pub domain: String,
    pub operation: String,
    pub total: usize,
    pub over_budget: usize,
    pub last_mean_ns: u64,
    pub min_mean_ns: u64,
    pub median_mean_ns: u64,
    pub max_mean_ns: u64,
    pub budget_ns: Option<u64>,
}

/// A surfaced problem: a failed execution or an over-budget benchmark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueEntry {
    pub kind: String,
    pub id: String,
    pub reference: String,
    pub detail: String,
}

/// A run that exceeded its slow threshold / latency budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlowEntry {
    pub kind: String,
    pub id: String,
    pub reference: String,
    pub observed_ns: u64,
    pub threshold_ns: u64,
}

/// The aggregated, deterministically-ordered summary of the test store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestStoreSummary {
    pub total_executions: usize,
    pub total_benchmarks: usize,
    pub last_run: Option<DateTime<Utc>>,
    pub validation_groups: Vec<ValidationGroupSummary>,
    pub benchmark_groups: Vec<BenchmarkGroupSummary>,
    pub issues: Vec<IssueEntry>,
    pub slow: Vec<SlowEntry>,
}

/// The generated artifacts, ready for the caller to write or diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestStoreIndexArtifacts {
    pub summary: TestStoreSummary,
    pub toon_sidecar: String,
    pub markdown: String,
    pub digest: String,
}

/// Generate the full test-store index from recorded executions and benchmarks.
pub fn generate_test_store_index(
    input: &TestStoreIndexInput<'_>
) -> TestStoreIndexArtifacts {
    let summary = aggregate_summary(input);

    let toon_sidecar =
        toon_format::encode_default(&summary).unwrap_or_else(|_| {
            serde_json::to_string(&summary).unwrap_or_default()
        });
    let digest = fnv1a_hex(toon_sidecar.as_bytes());
    let markdown = render_markdown(&summary, &digest);

    TestStoreIndexArtifacts {
        summary,
        toon_sidecar,
        markdown,
        digest,
    }
}

fn aggregate_summary(input: &TestStoreIndexInput<'_>) -> TestStoreSummary {
    let slow_thresholds: BTreeMap<&str, u64> = input
        .specs
        .iter()
        .filter_map(|spec| {
            spec.slow_threshold_ms.map(|ms| (spec.id.as_str(), ms))
        })
        .collect();

    // ── Validation groups ───────────────────────────────────────────────────
    let mut grouped: BTreeMap<String, Vec<&ValidationExecution>> =
        BTreeMap::new();
    for exec in input.executions {
        grouped
            .entry(exec.validation_spec_id.clone())
            .or_default()
            .push(exec);
    }

    let mut validation_groups = Vec::new();
    for (spec_id, mut execs) in grouped {
        execs.sort_by(|a, b| {
            a.executed_at.cmp(&b.executed_at).then(a.id.cmp(&b.id))
        });
        let latest = execs.last().expect("group is non-empty");
        let passed = execs.iter().filter(|e| e.outcome.is_passed()).count();
        let failed = execs.iter().filter(|e| e.outcome.is_failed()).count();
        let blocked = execs.iter().filter(|e| e.outcome.is_blocked()).count();

        let mut durations: Vec<u64> =
            execs.iter().filter_map(|e| e.duration_ms).collect();
        durations.sort_unstable();

        validation_groups.push(ValidationGroupSummary {
            validation_spec_id: spec_id,
            total: execs.len(),
            passed,
            failed,
            blocked,
            latest_outcome: outcome_label(&latest.outcome).to_string(),
            source_path: latest.provenance.source_path.clone(),
            test_id: latest.provenance.test_id.clone(),
            domain: latest.provenance.domain.clone(),
            operation: latest.provenance.operation.clone(),
            transport: latest.provenance.transport.clone(),
            run_id: latest.provenance.run_id.clone(),
            last_duration_ms: latest.duration_ms,
            min_duration_ms: durations.first().copied(),
            median_duration_ms: median(&durations),
            max_duration_ms: durations.last().copied(),
        });
    }

    // ── Benchmark groups ────────────────────────────────────────────────────
    let mut bgrouped: BTreeMap<String, Vec<&BenchmarkExecution>> =
        BTreeMap::new();
    for bench in input.benchmarks {
        let key = format!("{}.{}", bench.domain, bench.operation);
        bgrouped.entry(key).or_default().push(bench);
    }

    let mut benchmark_groups = Vec::new();
    for (key, mut benches) in bgrouped {
        benches.sort_by(|a, b| {
            a.executed_at.cmp(&b.executed_at).then(a.id.cmp(&b.id))
        });
        let latest = benches.last().expect("group is non-empty");
        let over_budget = benches.iter().filter(|b| b.over_budget).count();

        let mut means: Vec<u64> = benches.iter().map(|b| b.mean_ns).collect();
        means.sort_unstable();

        benchmark_groups.push(BenchmarkGroupSummary {
            key,
            domain: latest.domain.clone(),
            operation: latest.operation.clone(),
            total: benches.len(),
            over_budget,
            last_mean_ns: latest.mean_ns,
            min_mean_ns: means.first().copied().unwrap_or(0),
            median_mean_ns: median(&means).unwrap_or(0),
            max_mean_ns: means.last().copied().unwrap_or(0),
            budget_ns: latest.budget_ns,
        });
    }

    // ── Issues + slow ───────────────────────────────────────────────────────
    let mut issues = Vec::new();
    let mut slow = Vec::new();

    for exec in input.executions {
        if exec.outcome.is_failed() {
            issues.push(IssueEntry {
                kind: "execution".to_string(),
                id: exec.id.clone(),
                reference: exec.validation_spec_id.clone(),
                detail: exec.detail.clone().unwrap_or_default(),
            });
        }
        if let (Some(duration), Some(threshold)) = (
            exec.duration_ms,
            slow_thresholds
                .get(exec.validation_spec_id.as_str())
                .copied(),
        ) {
            if duration > threshold {
                slow.push(SlowEntry {
                    kind: "execution".to_string(),
                    id: exec.id.clone(),
                    reference: exec.validation_spec_id.clone(),
                    observed_ns: duration.saturating_mul(1_000_000),
                    threshold_ns: threshold.saturating_mul(1_000_000),
                });
            }
        }
    }

    for bench in input.benchmarks {
        if bench.over_budget {
            let key = format!("{}.{}", bench.domain, bench.operation);
            issues.push(IssueEntry {
                kind: "benchmark".to_string(),
                id: bench.id.clone(),
                reference: key.clone(),
                detail: format!(
                    "mean {} ns over budget {} ns",
                    bench.mean_ns,
                    bench.budget_ns.unwrap_or(0)
                ),
            });
            if let Some(budget) = bench.budget_ns {
                slow.push(SlowEntry {
                    kind: "benchmark".to_string(),
                    id: bench.id.clone(),
                    reference: key,
                    observed_ns: bench.mean_ns,
                    threshold_ns: budget,
                });
            }
        }
    }

    issues.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.id.cmp(&b.id)));
    slow.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.id.cmp(&b.id)));

    let last_run = input
        .executions
        .iter()
        .map(|e| e.executed_at)
        .chain(input.benchmarks.iter().map(|b| b.executed_at))
        .max();

    TestStoreSummary {
        total_executions: input.executions.len(),
        total_benchmarks: input.benchmarks.len(),
        last_run,
        validation_groups,
        benchmark_groups,
        issues,
        slow,
    }
}

fn render_markdown(
    summary: &TestStoreSummary,
    digest: &str,
) -> String {
    let mut out = String::new();
    out.push_str(TEST_INDEX_FILE_COMMENT);
    out.push('\n');
    out.push_str(&format!("<!-- test-index:digest {digest} -->\n\n"));
    out.push_str("# Test Store Index\n\n");
    out.push_str(&format!(
        "- total executions: {}\n- total benchmarks: {}\n- last run: {}\n\n",
        summary.total_executions,
        summary.total_benchmarks,
        summary
            .last_run
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| "none".to_string()),
    ));

    out.push_str("## Validation groups\n\n");
    if summary.validation_groups.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        out.push_str("| spec | total | pass | fail | blocked | latest | provenance | min/median/max ms |\n");
        out.push_str("|---|---|---|---|---|---|---|---|\n");
        for g in &summary.validation_groups {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {}/{}/{} |\n",
                g.validation_spec_id,
                g.total,
                g.passed,
                g.failed,
                g.blocked,
                g.latest_outcome,
                provenance_label(g),
                opt(g.min_duration_ms),
                opt(g.median_duration_ms),
                opt(g.max_duration_ms),
            ));
        }
        out.push('\n');
    }

    out.push_str("## Benchmark groups\n\n");
    if summary.benchmark_groups.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        out.push_str("| operation | total | over_budget | last mean ns | min/median/max ns | budget ns |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for g in &summary.benchmark_groups {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {}/{}/{} | {} |\n",
                g.key,
                g.total,
                g.over_budget,
                g.last_mean_ns,
                g.min_mean_ns,
                g.median_mean_ns,
                g.max_mean_ns,
                opt(g.budget_ns),
            ));
        }
        out.push('\n');
    }

    out.push_str("## Issues\n\n");
    if summary.issues.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        for issue in &summary.issues {
            out.push_str(&format!(
                "- [{}] {} ({}): {}\n",
                issue.kind, issue.id, issue.reference, issue.detail
            ));
        }
        out.push('\n');
    }

    out.push_str("## Slow\n\n");
    if summary.slow.is_empty() {
        out.push_str("_none_\n");
    } else {
        for entry in &summary.slow {
            out.push_str(&format!(
                "- [{}] {} ({}): observed {} ns > threshold {} ns\n",
                entry.kind,
                entry.id,
                entry.reference,
                entry.observed_ns,
                entry.threshold_ns
            ));
        }
    }

    out
}

fn opt(value: Option<u64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn provenance_label(group: &ValidationGroupSummary) -> String {
    let domain = group.domain.as_deref().unwrap_or("-");
    let operation = group.operation.as_deref().unwrap_or("-");
    let transport = group.transport.as_deref().unwrap_or("-");
    let run_id = group.run_id.as_deref().unwrap_or("-");
    format!("{domain}.{operation}@{transport}#{run_id}")
}

fn outcome_label(outcome: &ValidationOutcome) -> &'static str {
    match outcome {
        ValidationOutcome::Passed => "passed",
        ValidationOutcome::Failed => "failed",
        ValidationOutcome::Blocked => "blocked",
    }
}

/// Median of a pre-sorted slice (lower-middle for even counts).
fn median(sorted: &[u64]) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    Some(sorted[(sorted.len() - 1) / 2])
}

/// Deterministic FNV-1a 64-bit digest, rendered as lowercase hex.
fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::ValidationLinks;

    fn at(secs: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 28, 12, 0, secs)
            .single()
            .unwrap()
    }

    fn exec(
        id: &str,
        spec: &str,
        outcome: ValidationOutcome,
        ms: Option<u64>,
        secs: u32,
    ) -> ValidationExecution {
        let mut e = ValidationExecution::new(id, spec, outcome, at(secs));
        e.duration_ms = ms;
        e
    }

    #[test]
    fn aggregates_validation_counts_and_timings() {
        let execs = vec![
            exec("e1", "vt-a", ValidationOutcome::Passed, Some(10), 1),
            exec("e2", "vt-a", ValidationOutcome::Failed, Some(30), 2),
            exec("e3", "vt-a", ValidationOutcome::Passed, Some(20), 3),
        ];
        let input = TestStoreIndexInput {
            executions: &execs,
            specs: &[],
            benchmarks: &[],
        };

        let summary = generate_test_store_index(&input).summary;
        assert_eq!(summary.total_executions, 3);
        let g = &summary.validation_groups[0];
        assert_eq!(g.total, 3);
        assert_eq!(g.passed, 2);
        assert_eq!(g.failed, 1);
        assert_eq!(g.latest_outcome, "passed");
        assert_eq!(g.last_duration_ms, Some(20));
        assert_eq!(g.min_duration_ms, Some(10));
        assert_eq!(g.median_duration_ms, Some(20));
        assert_eq!(g.max_duration_ms, Some(30));
    }

    #[test]
    fn surfaces_failed_executions_and_slow_runs() {
        let mut spec = ValidationSpec::new("vt-a", "A");
        spec.slow_threshold_ms = Some(15);
        let specs = vec![spec];

        let mut failing =
            exec("e-fail", "vt-a", ValidationOutcome::Failed, Some(40), 2);
        failing.detail = Some("boom".to_string());
        failing.links = ValidationLinks {
            ticket_ids: vec!["t1".to_string()],
            ..Default::default()
        };
        let execs = vec![
            exec("e-ok", "vt-a", ValidationOutcome::Passed, Some(10), 1),
            failing,
        ];

        let input = TestStoreIndexInput {
            executions: &execs,
            specs: &specs,
            benchmarks: &[],
        };
        let summary = generate_test_store_index(&input).summary;

        assert_eq!(summary.issues.len(), 1);
        assert_eq!(summary.issues[0].id, "e-fail");
        assert_eq!(summary.issues[0].detail, "boom");

        // e-fail duration 40ms > 15ms threshold → slow
        assert_eq!(summary.slow.len(), 1);
        assert_eq!(summary.slow[0].id, "e-fail");
        assert_eq!(summary.slow[0].observed_ns, 40_000_000);
        assert_eq!(summary.slow[0].threshold_ns, 15_000_000);
    }

    #[test]
    fn surfaces_over_budget_benchmarks_in_issues_and_slow() {
        let mut bench =
            BenchmarkExecution::new("b1", "get_by_id", "get", "ticket", at(5));
        bench.mean_ns = 75_000_000;
        bench.apply_budget(Some(50_000_000));

        let input = TestStoreIndexInput {
            executions: &[],
            specs: &[],
            benchmarks: std::slice::from_ref(&bench),
        };
        let summary = generate_test_store_index(&input).summary;

        assert_eq!(summary.benchmark_groups.len(), 1);
        assert_eq!(summary.benchmark_groups[0].key, "ticket.get");
        assert_eq!(summary.benchmark_groups[0].over_budget, 1);
        assert_eq!(summary.issues.len(), 1);
        assert_eq!(summary.issues[0].kind, "benchmark");
        assert_eq!(summary.slow.len(), 1);
        assert_eq!(summary.slow[0].observed_ns, 75_000_000);
        assert_eq!(summary.slow[0].threshold_ns, 50_000_000);
    }

    #[test]
    fn generation_is_deterministic_for_same_input() {
        let execs = vec![
            exec("e1", "vt-a", ValidationOutcome::Passed, Some(10), 1),
            exec("e2", "vt-b", ValidationOutcome::Blocked, None, 2),
        ];
        let input = TestStoreIndexInput {
            executions: &execs,
            specs: &[],
            benchmarks: &[],
        };

        let first = generate_test_store_index(&input);
        let second = generate_test_store_index(&input);
        assert_eq!(first.digest, second.digest);
        assert_eq!(first.markdown, second.markdown);
        assert_eq!(first.toon_sidecar, second.toon_sidecar);
        assert!(first.markdown.contains(&first.digest));
    }
}
