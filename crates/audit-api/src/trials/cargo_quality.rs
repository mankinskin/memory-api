use std::io::Cursor;
use std::path::Path;
use std::process::{
    Command,
    Output,
};

use cargo_metadata::{
    MetadataCommand,
    Message,
    diagnostic::DiagnosticLevel,
};
use serde::Deserialize;
use serde_json::{
    Value,
    json,
};

use crate::error::AuditError;
use crate::config::{
    normalize_output_text,
    is_repo_relative_path_excluded,
    normalize_repo_relative_path,
};
use crate::models::{
    AuditFinding,
    CountMetric,
    CoverageSummary,
    Severity,
    TestSummary,
    TrialStatus,
};

pub struct CountTrialResult {
    pub metric: CountMetric,
    pub findings: Vec<AuditFinding>,
}

pub struct TestTrialResult {
    pub metric: TestSummary,
    pub findings: Vec<AuditFinding>,
}

pub struct CoverageTrialResult {
    pub metric: CoverageSummary,
    pub findings: Vec<AuditFinding>,
}

pub fn collect_compiler_warnings(
    repo_root: &Path,
    exclude_paths: &[String],
) -> Result<CountTrialResult, AuditError> {
    let cargo_scope = cargo_scope(repo_root, exclude_paths)?;
    if !cargo_scope.has_manifest {
        return Ok(CountTrialResult {
            metric: CountMetric {
                status: TrialStatus::NotApplicable,
                count: None,
                details: Some("No Cargo.toml found at the repository root.".to_string()),
            },
            findings: Vec::new(),
        });
    }

    if cargo_scope.package_names.is_empty() {
        return Ok(CountTrialResult {
            metric: CountMetric {
                status: TrialStatus::NotApplicable,
                count: None,
                details: Some("All workspace Cargo packages are excluded by config.".to_string()),
            },
            findings: Vec::new(),
        });
    }

    let mut args = vec![
        "check".to_string(),
        "--all-targets".to_string(),
        "--message-format=json-diagnostic-rendered-ansi".to_string(),
    ];
    append_package_args(&mut args, &cargo_scope.package_names);

    let output = run_command(repo_root, "cargo", args)?;

    let mut warnings = Vec::new();
    for message in Message::parse_stream(Cursor::new(&output.stdout)) {
        let Ok(message) = message else {
            continue;
        };
        if let Message::CompilerMessage(compiler_message) = message {
            if compiler_message.message.level == DiagnosticLevel::Warning {
                let primary_span = compiler_message
                    .message
                    .spans
                    .iter()
                    .find(|span| span.is_primary);
                if primary_span.is_some_and(|span| {
                    is_file_name_excluded(repo_root, &span.file_name, exclude_paths)
                }) {
                    continue;
                }
                warnings.push(json!({
                    "message": compiler_message.message.message,
                    "code": compiler_message.message.code.as_ref().map(|code| code.code.clone()),
                    "path": primary_span.map(|span| normalize_output_text(&span.file_name)),
                    "line": primary_span.map(|span| span.line_start),
                    "rendered": compiler_message
                        .message
                        .rendered
                        .as_deref()
                        .map(normalize_output_text),
                }));
            }
        }
    }

    let mut findings = Vec::new();
    if !warnings.is_empty() {
        findings.push(AuditFinding {
            id: "compiler_warnings".to_string(),
            category: "compiler_warning".to_string(),
            severity: if warnings.len() > 20 {
                Severity::High
            } else {
                Severity::Medium
            },
            summary: format!(
                "cargo check reported {} compiler warnings.",
                warnings.len()
            ),
            path: None,
            line: None,
            metric_name: "compiler_warning_count".to_string(),
            metric_value: json!(warnings.len()),
            threshold: Some(json!(0)),
            instructions: vec![
                "Fix compiler warnings before adding more changes so dead code, unused variables, and deprecations do not accumulate.".to_string(),
                "Re-run `cargo check --workspace --all-targets` after each warning batch to keep the workspace clean.".to_string(),
            ],
            evidence: json!({
                "warning_count": warnings.len(),
                "sample": warnings.iter().take(20).cloned().collect::<Vec<_>>(),
            }),
        });
    }

    if !output.status.success() {
        findings.push(AuditFinding {
            id: "compiler_check_failed".to_string(),
            category: "compiler_check".to_string(),
            severity: Severity::High,
            summary: "cargo check failed, so warning counts may be incomplete.".to_string(),
            path: None,
            line: None,
            metric_name: "cargo_check_exit_code".to_string(),
            metric_value: json!(output.status.code()),
            threshold: None,
            instructions: vec![
                "Fix build errors first, then rerun the audit so compiler warnings can be reported accurately.".to_string(),
            ],
            evidence: json!({
                "stderr": trim_output(&output.stderr),
            }),
        });
    }

    Ok(CountTrialResult {
        metric: CountMetric {
            status: if output.status.success() {
                TrialStatus::Collected
            } else {
                TrialStatus::Failed
            },
            count: Some(warnings.len()),
            details: if output.status.success() {
                None
            } else {
                Some(trim_output(&output.stderr))
            },
        },
        findings,
    })
}

pub fn collect_test_success(
    repo_root: &Path,
    exclude_paths: &[String],
) -> Result<TestTrialResult, AuditError> {
    let cargo_scope = cargo_scope(repo_root, exclude_paths)?;
    if !cargo_scope.has_manifest {
        return Ok(TestTrialResult {
            metric: TestSummary {
                status: TrialStatus::NotApplicable,
                total: None,
                passed: None,
                failed: None,
                ignored: None,
                success_rate: None,
                details: Some("No Cargo.toml found at the repository root.".to_string()),
            },
            findings: Vec::new(),
        });
    }

    if cargo_scope.package_names.is_empty() {
        return Ok(TestTrialResult {
            metric: TestSummary {
                status: TrialStatus::NotApplicable,
                total: None,
                passed: None,
                failed: None,
                ignored: None,
                success_rate: None,
                details: Some("All workspace Cargo packages are excluded by config.".to_string()),
            },
            findings: Vec::new(),
        });
    }

    let mut args = vec![
        "test".to_string(),
        "--lib".to_string(),
        "--tests".to_string(),
        "--no-fail-fast".to_string(),
    ];
    append_package_args(&mut args, &cargo_scope.package_names);
    args.extend([
        "--".to_string(),
        "--format=json".to_string(),
        "-Z".to_string(),
        "unstable-options".to_string(),
    ]);

    let output = run_command(repo_root, "cargo", args)?;

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut ignored = 0usize;
    let mut failing_tests = Vec::new();

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(event) = serde_json::from_str::<LibtestEvent>(line) else {
            continue;
        };

        if event.kind != "test" {
            continue;
        }

        match event.event.as_deref() {
            Some("ok") => passed += 1,
            Some("failed") => {
                failed += 1;
                if let Some(name) = event.name {
                    failing_tests.push(name);
                }
            },
            Some("ignored") => ignored += 1,
            _ => {},
        }
    }

    let total = passed + failed + ignored;
    let success_rate = if passed + failed == 0 {
        None
    } else {
        Some((passed as f64 / (passed + failed) as f64) * 100.0)
    };

    let mut findings = Vec::new();
    if failed > 0 {
        findings.push(AuditFinding {
            id: "test_failures".to_string(),
            category: "test_failure".to_string(),
            severity: Severity::High,
            summary: format!(
                "cargo test reported {} failing tests out of {} executed tests.",
                failed,
                passed + failed
            ),
            path: None,
            line: None,
            metric_name: "test_success_rate".to_string(),
            metric_value: json!(success_rate),
            threshold: Some(json!(100.0)),
            instructions: vec![
                "Fix failing tests before trusting the rest of the quality metrics.".to_string(),
                "Re-run the failing test names directly so you can stabilize the smallest broken slice first.".to_string(),
            ],
            evidence: json!({
                "passed": passed,
                "failed": failed,
                "ignored": ignored,
                "failing_tests": failing_tests.iter().take(20).cloned().collect::<Vec<_>>(),
            }),
        });
    }

    if !output.status.success() && failed == 0 {
        findings.push(AuditFinding {
            id: "test_command_failed".to_string(),
            category: "test_execution".to_string(),
            severity: Severity::High,
            summary: "cargo test failed before structured test results could be collected.".to_string(),
            path: None,
            line: None,
            metric_name: "cargo_test_exit_code".to_string(),
            metric_value: json!(output.status.code()),
            threshold: None,
            instructions: vec![
                "Fix the cargo test invocation or build failure and rerun the audit to restore test success metrics.".to_string(),
            ],
            evidence: json!({
                "stderr": trim_output(&output.stderr),
            }),
        });
    }

    Ok(TestTrialResult {
        metric: TestSummary {
            status: if total > 0 {
                TrialStatus::Collected
            } else if output.status.success() {
                TrialStatus::Collected
            } else {
                TrialStatus::Failed
            },
            total: Some(total),
            passed: Some(passed),
            failed: Some(failed),
            ignored: Some(ignored),
            success_rate,
            details: if output.status.success() {
                None
            } else {
                Some(trim_output(&output.stderr))
            },
        },
        findings,
    })
}

pub fn collect_coverage(
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
                details: Some("No Cargo.toml found at the repository root.".to_string()),
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
                details: Some("All workspace Cargo packages are excluded by config.".to_string()),
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
    let line_percent = lines
        .get("percent")
        .and_then(Value::as_f64)
        .or_else(|| match (covered_lines, total_lines) {
            (Some(covered_lines), Some(total_lines)) if total_lines > 0 => {
                Some((covered_lines as f64 / total_lines as f64) * 100.0)
            },
            _ => None,
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

#[derive(Debug, Deserialize)]
struct LibtestEvent {
    #[serde(rename = "type")]
    kind: String,
    event: Option<String>,
    name: Option<String>,
}

fn run_command(
    repo_root: &Path,
    program: &str,
    args: Vec<String>,
) -> Result<Output, AuditError> {
    let output = Command::new(program)
        .args(args)
        .current_dir(repo_root)
        .output()?;
    Ok(output)
}

fn has_cargo_manifest(repo_root: &Path) -> bool {
    repo_root.join("Cargo.toml").exists()
}

fn cargo_scope(
    repo_root: &Path,
    exclude_paths: &[String],
) -> Result<CargoScope, AuditError> {
    if !has_cargo_manifest(repo_root) {
        return Ok(CargoScope {
            has_manifest: false,
            package_names: Vec::new(),
        });
    }

    let metadata = MetadataCommand::new()
        .current_dir(repo_root)
        .no_deps()
        .exec()
        .map_err(|err| AuditError::CommandFailed {
            command: "cargo metadata --no-deps".to_string(),
            details: normalize_output_text(err.to_string()),
        })?;

    let package_names = metadata
        .workspace_packages()
        .iter()
        .filter_map(|package| {
            let manifest_path = package.manifest_path.as_std_path().canonicalize().ok()?;
            let relative_manifest = manifest_path.strip_prefix(repo_root).ok()?;
            if is_repo_relative_path_excluded(relative_manifest, exclude_paths) {
                return None;
            }
            Some(package.name.to_string())
        })
        .collect();

    Ok(CargoScope {
        has_manifest: true,
        package_names,
    })
}

fn append_package_args(
    args: &mut Vec<String>,
    package_names: &[String],
) {
    for package_name in package_names {
        args.push("-p".to_string());
        args.push(package_name.clone());
    }
}

fn is_file_name_excluded(
    repo_root: &Path,
    file_name: &str,
    exclude_paths: &[String],
) -> bool {
    let file_path = Path::new(file_name);
    let relative = file_path
        .strip_prefix(repo_root)
        .ok()
        .map(normalize_repo_relative_path)
        .or_else(|| {
            if file_path.is_relative() {
                Some(normalize_repo_relative_path(file_path))
            } else {
                None
            }
        });

    relative
        .as_deref()
        .is_some_and(|relative| is_repo_relative_path_excluded(Path::new(relative), exclude_paths))
}

struct CargoScope {
    has_manifest: bool,
    package_names: Vec<String>,
}

fn trim_output(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    let lines = text.lines().take(40).collect::<Vec<_>>();
    normalize_output_text(lines.join("\n"))
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