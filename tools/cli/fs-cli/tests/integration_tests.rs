//! Integration tests for fs CLI.

use std::{
    fs,
    path::Path,
};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn test_list_dir_basic() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create some test files
    fs::write(dir_path.join("file1.txt"), "content1").unwrap();
    fs::write(dir_path.join("file2.rs"), "content2").unwrap();
    fs::create_dir(dir_path.join("subdir")).unwrap();

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("list-dir").arg(dir_path).arg("--json");

    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let entries = json["entries"].as_array().unwrap();
    assert!(entries.len() >= 3);
}

#[test]
fn test_list_dir_with_limit() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create many files to trigger truncation
    for i in 0..100 {
        fs::write(dir_path.join(format!("file{}.txt", i)), "content").unwrap();
    }

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("list-dir")
        .arg(dir_path)
        .arg("--limit")
        .arg("10")
        .arg("--json");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let entries = json["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 10);
    assert_eq!(json["truncated"], true);
}

#[test]
fn test_stat_existing_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "content").unwrap();

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("stat").arg(&file_path).arg("--json");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["exists"], true);
    assert_eq!(json["kind"], "file");
}

#[test]
fn test_stat_missing_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("nonexistent.txt");

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("stat").arg(&file_path).arg("--json");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["exists"], false);
}

#[test]
fn test_move_file_conflict() {
    let temp_dir = TempDir::new().unwrap();
    let src = temp_dir.path().join("src.txt");
    let dst = temp_dir.path().join("dst.txt");

    fs::write(&src, "source content").unwrap();
    fs::write(&dst, "destination content").unwrap();

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("move")
        .arg(&src)
        .arg(&dst)
        .arg("--root")
        .arg(temp_dir.path())
        .arg("--json");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let conflicts = json["conflicts"].as_array().unwrap();
    assert!(!conflicts.is_empty());

    let conflict = &conflicts[0];
    assert_eq!(conflict["kind"], "destination_exists");
}

#[test]
fn test_move_file_with_overwrite() {
    let temp_dir = TempDir::new().unwrap();
    let src = temp_dir.path().join("src.txt");
    let dst = temp_dir.path().join("dst.txt");

    fs::write(&src, "source content").unwrap();
    fs::write(&dst, "destination content").unwrap();

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("move")
        .arg(&src)
        .arg(&dst)
        .arg("--overwrite")
        .arg("--root")
        .arg(temp_dir.path())
        .arg("--json");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let conflicts = json["conflicts"].as_array().unwrap();
    assert!(conflicts.is_empty());
    assert!(!src.exists());
    assert!(dst.exists());
}

#[test]
fn test_copy_file() {
    let temp_dir = TempDir::new().unwrap();
    let src = temp_dir.path().join("src.txt");
    let dst = temp_dir.path().join("dst.txt");

    fs::write(&src, "content").unwrap();

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("copy")
        .arg(&src)
        .arg(&dst)
        .arg("--root")
        .arg(temp_dir.path())
        .arg("--json");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    assert!(src.exists());
    assert!(dst.exists());
}

#[test]
fn test_delete_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "content").unwrap();

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("delete-file")
        .arg(&file_path)
        .arg("--root")
        .arg(temp_dir.path())
        .arg("--json");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    assert!(!file_path.exists());
}

#[test]
fn test_delete_dir_recursive() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path().join("subdir");
    fs::create_dir(&dir_path).unwrap();
    fs::write(dir_path.join("file.txt"), "content").unwrap();

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("delete-dir")
        .arg(&dir_path)
        .arg("--recursive")
        .arg("--root")
        .arg(temp_dir.path())
        .arg("--json");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    assert!(!dir_path.exists());
}

#[test]
fn test_toon_output_format() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "content").unwrap();

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("stat").arg(&file_path).arg("--toon");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    // TOON output should be compact and parseable
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty());
    // Basic TOON validation - it should not look like JSON
    assert!(!stdout.trim_start().starts_with('{'));
}

// ── Security Validation Tests ───────────────────────────────────────────────

#[test]
#[cfg_attr(windows, ignore = "Requires elevated privileges on Windows")]
fn test_security_symlink_escape_via_move() {
    let root_dir = TempDir::new().unwrap();
    let outside_dir = TempDir::new().unwrap();

    let safe_file = root_dir.path().join("safe.txt");
    fs::write(&safe_file, "safe content").unwrap();

    // Create symlink pointing outside root
    let symlink_path = root_dir.path().join("escape_link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside_dir.path(), &symlink_path).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(outside_dir.path(), &symlink_path)
        .unwrap();

    let escape_dest = symlink_path.join("escaped.txt");

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("move")
        .arg(&safe_file)
        .arg(&escape_dest)
        .arg("--root")
        .arg(root_dir.path())
        .arg("--json");

    let output = cmd.output().unwrap();
    // Should fail - symlink escape detected
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("PathTraversal") || stderr.contains("path traversal")
    );
}

#[test]
fn test_security_parent_directory_escape_via_delete() {
    let root_dir = TempDir::new().unwrap();
    let nested = root_dir.path().join("nested");
    fs::create_dir(&nested).unwrap();

    // Try to delete using ../ to escape root
    let escape_path = nested.join("..").join("..").join("etc").join("passwd");

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("delete-file")
        .arg(&escape_path)
        .arg("--root")
        .arg(root_dir.path())
        .arg("--json");

    let output = cmd.output().unwrap();
    // Should fail - parent directory escape detected
    assert!(!output.status.success());
}

#[test]
fn test_security_copy_with_valid_root() {
    let root_dir = TempDir::new().unwrap();
    let src = root_dir.path().join("src.txt");
    let dst = root_dir.path().join("subdir").join("dst.txt");

    fs::write(&src, "content").unwrap();
    fs::create_dir(root_dir.path().join("subdir")).unwrap();

    let mut cmd = Command::cargo_bin("fs").unwrap();
    cmd.arg("copy")
        .arg(&src)
        .arg(&dst)
        .arg("--root")
        .arg(root_dir.path())
        .arg("--json");

    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Both files should exist
    assert!(src.exists());
    assert!(dst.exists());
}
