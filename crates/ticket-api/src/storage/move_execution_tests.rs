use super::*;

use super::*;
use crate::{
    model::filesystem::ScanRoot,
    storage::{
        index::RedbIndexStore,
        move_planner::MovePreflightBlocker,
        BoardEntryStatus,
    },
};
use chrono::Utc;
use std::process::Command;
use tempfile::tempdir;

fn run_git(repo_root: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .status()
        .expect("git command");
    assert!(status.success(), "git {args:?} failed: {status}");
}

fn git_commit_path(
    repo_root: &std::path::Path,
    pathspec: &str,
    message: &str,
) {
    run_git(repo_root, &["config", "user.name", "Move Test"]);
    run_git(repo_root, &["config", "user.email", "move-test@example.com"]);
    run_git(repo_root, &["add", "--", pathspec]);
    run_git(repo_root, &["commit", "-m", message]);
}

#[path = "move_execution_tests/tests_core.rs"]
mod tests_core;
#[path = "move_execution_tests/tests_resume_rollback.rs"]
mod tests_resume_rollback;
#[path = "move_execution_tests/tests_sequential_rewrites.rs"]
mod tests_sequential_rewrites;
