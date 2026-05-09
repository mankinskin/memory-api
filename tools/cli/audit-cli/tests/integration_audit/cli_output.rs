use assert_cmd::Command;
use audit_cli::cli::{CliOutput, parse_cli_from, run};
use tempfile::tempdir;

use super::fixtures::{
    assert_unix_formatted_output_text, assert_unix_formatted_output_value, write_sample_repo,
};

#[test]
fn cli_supports_json_and_text_output() {
    let repo = tempdir().expect("temp repo");
    write_sample_repo(repo.path());

    let cli = parse_cli_from([
        "audit",
        "run",
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
        }
        CliOutput::Text(_) => panic!("expected json output"),
    }

    let text_cli = parse_cli_from([
        "audit",
        "run",
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
        .arg("run")
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
fn cli_without_subcommand_shows_help() {
    let mut command = Command::cargo_bin("audit").expect("audit binary");
    command
        .assert()
        .success()
        .stdout(predicates::str::contains("Repository quality audit CLI"))
        .stdout(predicates::str::contains("run"));
}