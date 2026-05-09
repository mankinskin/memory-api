use std::{
    path::Path,
    process::Command,
};

use serde_json::{
    Value,
    json,
};

use crate::{
    error::AuditError,
    models::{
        AuditFinding,
        CoverageSummary,
        Severity,
        TrialStatus,
    },
};

use super::{
    CoverageTrialResult,
    append_package_args,
    cargo_scope,
    run_command,
    trim_output,
};

pub(super) fn collect_coverage(
    repo_root: &Path,
    exclude_paths: &[String],
    warn_below: f64,
) -> Result<CoverageTrialResult, AuditError> {
    let cargo_scope = cargo_scope(repo_root, exclude_paths)?;
    if !cargo_scope.has_manifest {
        return Ok(CoverageTrialResult {
            metric: CoverageSummary {
                status: TrialStatus::NotApplicable,
                line_percent: None,
                covered_lines: None,
                total_lines: None,
                details: Some(
                    "No Cargo.toml found at the repository root.".to_string(),
                ),
            },
            findings: Vec::new(),
        });
    }

    if cargo_scope.package_names.is_empty() {
        return Ok(CoverageTrialResult {
            metric: CoverageSummary {
                status: TrialStatus::NotApplicable,
                line_percent: None,
                covered_lines: None,
                total_lines: None,
                details: Some(
                    "All workspace Cargo packages are excluded by config."
                        .to_string(),
                ),
            },
            findings: Vec::new(),
        });
    }

    let version_probe = Command::new("cargo")
        .arg("llvm-cov")
        .arg("--version")
        .current_dir(repo_root)
        .output();

    let Ok(version_probe) = version_probe else {
        return Ok(missing_coverage_tool_result());
    };
    if !version_probe.status.success() {
        return Ok(missing_coverage_tool_result());
    }

    let mut args = vec!["llvm-cov".to_string(), "--json-summary".to_string()];
    append_package_args(&mut args, &cargo_scope.package_names);
    let output = run_command(repo_root, "cargo", args)?;

    if !output.status.success() {
        return Ok(CoverageTrialResult {
            metric: CoverageSummary {
                status: TrialStatus::Failed,
                line_percent: None,
                covered_lines: None,
                total_lines: None,
                details: Some(trim_output(&output.stderr)),
            },
            findings: vec![AuditFinding {
                id: "coverage_collection_failed".to_string(),
                category: "coverage".to_string(),
                severity: Severity::High,
                summary: "cargo llvm-cov failed, so coverage metrics could not be collected.".to_string(),
                path: None,
                line: None,
                metric_name: "coverage_status".to_string(),
                metric_value: json!("failed"),
                threshold: Some(json!(warn_below)),
                instructions: vec![
                    "Fix the failing coverage command, then rerun the audit so line coverage can be measured.".to_string(),
                ],
                evidence: json!({
                    "stderr": trim_output(&output.stderr),
                }),
            }],
        });
    }

    let json: Value = serde_json::from_slice(&output.stdout)?;
    let lines = json
        .pointer("/totals/lines")
        .or_else(|| json.pointer("/data/0/totals/lines"));

    let Some(lines) = lines else {
        return Ok(CoverageTrialResult {
            metric: CoverageSummary {
                status: TrialStatus::Failed,
                line_percent: None,
                covered_lines: None,
                total_lines: None,
                details: Some("Could not parse the coverage summary emitted by cargo llvm-cov.".to_string()),
            },
            findings: vec![AuditFinding {
                id: "coverage_parse_failed".to_string(),
                category: "coverage".to_string(),
                severity: Severity::Medium,
                summary: "Coverage output was present but did not match the expected summary format.".to_string(),
                path: None,
                line: None,
                metric_name: "coverage_status".to_string(),
                metric_value: json!("unparsed"),
                threshold: Some(json!(warn_below)),
                instructions: vec![
                    "Check the installed cargo-llvm-cov version and update the parser if its JSON summary format changed.".to_string(),
                ],
                evidence: json!({
                    "stdout": trim_output(&output.stdout),
                }),
            }],
        });
    };

    let covered_lines = lines
        .get("covered")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let total_lines = lines
        .get("count")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let line_percent =
        lines.get("percent").and_then(Value::as_f64).or_else(|| {
            match (covered_lines, total_lines) {
                (Some(covered_lines), Some(total_lines)) if total_lines > 0 =>
                    Some((covered_lines as f64 / total_lines as f64) * 100.0),
                _ => None,
            }
        });

    let mut findings = Vec::new();
    if let Some(line_percent) = line_percent {
        if line_percent < warn_below {
            findings.push(AuditFinding {
                id: "coverage_below_threshold".to_string(),
                category: "coverage".to_string(),
                severity: Severity::Medium,
                summary: format!(
                    "Line coverage is {:.1}%, below the {:.1}% target.",
                    line_percent, warn_below
                ),
                path: None,
                line: None,
                metric_name: "line_coverage_percent".to_string(),
                metric_value: json!(line_percent),
                threshold: Some(json!(warn_below)),
                instructions: vec![
                    "Add focused unit tests around the highest-risk modules until coverage clears the configured target.".to_string(),
                    "Prefer small branch-specific tests over broad integration tests when closing coverage gaps.".to_string(),
                ],
                evidence: json!({
                    "line_percent": line_percent,
                    "covered_lines": covered_lines,
                    "total_lines": total_lines,
                }),
            });
        }
    }

    Ok(CoverageTrialResult {
        metric: CoverageSummary {
            status: TrialStatus::Collected,
            line_percent,
            covered_lines,
            total_lines,
            details: None,
        },
        findings,
    })
}

fn missing_coverage_tool_result() -> CoverageTrialResult {
    CoverageTrialResult {
        metric: CoverageSummary {
            status: TrialStatus::Unavailable,
            line_percent: None,
            covered_lines: None,
            total_lines: None,
            details: Some("cargo llvm-cov is not installed in this environment.".to_string()),
        },
        findings: vec![AuditFinding {
            id: "coverage_tool_missing".to_string(),
            category: "coverage".to_string(),
            severity: Severity::Medium,
            summary: "Coverage metrics are unavailable because cargo llvm-cov is not installed.".to_string(),
            path: None,
            line: None,
            metric_name: "coverage_status".to_string(),
            metric_value: json!("unavailable"),
            threshold: None,
            instructions: vec![
                "Install cargo-llvm-cov in the audit environment so line coverage can be collected automatically.".to_string(),
                "After installation, rerun the audit to populate coverage metrics and threshold findings.".to_string(),
            ],
            evidence: json!({
                "command": "cargo llvm-cov --version",
            }),
        }],
    }
}
