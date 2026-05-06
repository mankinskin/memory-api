use std::fs;
use std::path::Path;

use assert_cmd::Command;
use audit_api::audit::audit;
use audit_api::models::{
    AuditConfig,
    TrialStatus,
};
use audit_cli::cli::{
    CliOutput,
    parse_cli_from,
    run,
};
use rusqlite::{
    Connection,
    params,
};
use tempfile::tempdir;

#[test]
fn audit_collects_findings_and_prunes_stale_index_entries() {
    let repo = tempdir().expect("temp repo");
    write_sample_repo(repo.path());

    let report = audit(
        repo.path(),
        AuditConfig {
            max_file_lines: 20,
            max_cyclomatic_complexity: 3,
            coverage_warn_below: 80.0,
        },
    )
    .expect("first audit succeeds");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.category == "file_length" && finding.path.as_deref() == Some("src/lib.rs"))
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.category == "static_complexity" && finding.path.as_deref() == Some("src/lib.rs"))
    );
    assert!(report.metrics.compiler_warnings.count.unwrap_or_default() >= 1);
    assert_eq!(report.metrics.test_results.failed, Some(0));
    assert!(report.metrics.test_results.passed.unwrap_or_default() >= 1);
    assert_eq!(report.metrics.test_results.success_rate, Some(100.0));

    match report.metrics.coverage.status {
        TrialStatus::Collected => {
            assert!(report.metrics.coverage.line_percent.is_some());
        },
        TrialStatus::Unavailable => {
            assert!(report.findings.iter().any(|finding| finding.id == "coverage_tool_missing"));
        },
        TrialStatus::Failed => {
            panic!("coverage collection should either succeed or report the tool as unavailable");
        },
        TrialStatus::NotApplicable => {
            panic!("coverage should be applicable for a Cargo repository");
        },
    }

    fs::remove_file(repo.path().join("src/extra.rs")).expect("remove stale file");

    let second_report = audit(
        repo.path(),
        AuditConfig {
            max_file_lines: 20,
            max_cyclomatic_complexity: 3,
            coverage_warn_below: 80.0,
        },
    )
    .expect("second audit succeeds");

    assert_eq!(second_report.sync.pruned_files, 1);

    let connection = Connection::open(&second_report.index_database).expect("open audit db");
    let indexed_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = ?1",
            params!["src/extra.rs"],
            |row| row.get(0),
        )
        .expect("query stale row count");
    assert_eq!(indexed_count, 0);
}

fn write_sample_repo(repo_root: &Path) {
    fs::create_dir_all(repo_root.join("src")).expect("create src dir");
    fs::write(
        repo_root.join("Cargo.toml"),
        r#"[package]
name = "sample-quality-repo"
version = "0.1.0"
edition = "2021"
"#,
    )
    .expect("write Cargo.toml");

    fs::write(
        repo_root.join("src/lib.rs"),
        r#"pub fn complicated(a: bool, b: bool, c: bool, d: bool) -> usize {
    let never_used = 7;

    if a && b {
        if c || d {
            return 1;
        }
    }

    if a {
        return 2;
    }

    if b {
        return 3;
    }

    if c {
        return 4;
    }

    if d {
        return 5;
    }

    0
}

pub fn line_padding() {
    let _ = 1;
}

pub fn more_padding() {
    let _ = 2;
}

pub fn even_more_padding() {
    let _ = 3;
}

#[cfg(test)]
mod tests {
    use super::complicated;

    #[test]
    fn complicated_returns_expected_branch() {
        assert_eq!(complicated(false, false, true, false), 4);
    }
}
"#,
    )
    .expect("write lib.rs");

    fs::write(
        repo_root.join("src/extra.rs"),
        "pub fn helper() -> usize { 1 }\n",
    )
    .expect("write extra.rs");
}

#[test]
fn cli_supports_json_and_text_output() {
    let repo = tempdir().expect("temp repo");
    write_sample_repo(repo.path());

    let cli = parse_cli_from([
        "audit",
        repo.path().to_string_lossy().as_ref(),
        "--json",
        "--max-file-lines",
        "20",
        "--max-cyclomatic-complexity",
        "3",
    ])
    .expect("parse cli");

    match run(cli).expect("run cli") {
        CliOutput::Json(value) => {
            assert_eq!(value["service"], "audit-mcp");
            assert!(value["findings"].as_array().is_some_and(|findings| !findings.is_empty()));
            assert_unix_formatted_output_value(&value["repo_root"]);
            assert_unix_formatted_output_value(&value["index_database"]);
            let compiler_warning = value["findings"]
                .as_array()
                .and_then(|findings| {
                    findings.iter().find(|finding| finding["category"] == "compiler_warning")
                })
                .expect("compiler warning finding");
            assert_unix_formatted_output_value(&compiler_warning["evidence"]["sample"][0]["path"]);
        },
        CliOutput::Text(_) => panic!("expected json output"),
    }

    let text_cli = parse_cli_from([
        "audit",
        repo.path().to_string_lossy().as_ref(),
        "--max-file-lines",
        "20",
        "--max-cyclomatic-complexity",
        "3",
    ])
    .expect("parse text cli");

    match run(text_cli).expect("run text cli") {
        CliOutput::Text(output) => assert_unix_formatted_output_text(&output),
        CliOutput::Json(_) => panic!("expected text output"),
    }

    let mut command = Command::cargo_bin("audit").expect("audit binary");
    command
        .arg(repo.path())
        .arg("--max-file-lines")
        .arg("20")
        .arg("--max-cyclomatic-complexity")
        .arg("3");
    command
        .assert()
        .success()
        .stdout(predicates::str::contains("Repository Audit"));
}

#[test]
fn config_excludes_paths_from_index_and_findings() {
    let repo = tempdir().expect("temp repo");
    fs::create_dir_all(repo.path().join("src")).expect("create src dir");
    fs::create_dir_all(repo.path().join("crates/deps/third_party/src"))
        .expect("create excluded dir");
    fs::write(
        repo.path().join("Cargo.toml"),
        r#"[package]
name = "sample-exclude-repo"
version = "0.1.0"
edition = "2021"
"#,
    )
    .expect("write Cargo.toml");
    fs::write(
        repo.path().join(".audit.toml"),
        "exclude_paths = [\"crates/deps/\"]\n",
    )
    .expect("write config");
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn short() -> usize { 1 }\n",
    )
    .expect("write src/lib.rs");
    fs::write(
        repo.path().join("crates/deps/third_party/src/lib.rs"),
        "pub fn very_long() -> usize {\n    1\n}\n\npub fn a() {}\npub fn b() {}\npub fn c() {}\npub fn d() {}\n",
    )
    .expect("write excluded file");

    let report = audit(
        repo.path(),
        AuditConfig {
            max_file_lines: 2,
            max_cyclomatic_complexity: 1,
            coverage_warn_below: 80.0,
        },
    )
    .expect("audit succeeds");

    assert!(!report.findings.iter().any(|finding| {
        finding.path.as_deref() == Some("crates/deps/third_party/src/lib.rs")
    }));

    let connection = Connection::open(&report.index_database).expect("open audit db");
    let indexed_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = ?1",
            params!["crates/deps/third_party/src/lib.rs"],
            |row| row.get(0),
        )
        .expect("query excluded row count");
    assert_eq!(indexed_count, 0);
}

fn assert_unix_formatted_output_value(value: &serde_json::Value) {
    let text = value.as_str().expect("string output value");
    assert_unix_formatted_output_text(text);
}

fn assert_unix_formatted_output_text(text: &str) {
    assert!(!text.contains('\\'), "expected Unix-style path separators: {text}");
    assert!(!text.contains("//?/"), "expected no Windows extended path prefix: {text}");
}