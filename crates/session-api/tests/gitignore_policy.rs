// Test gitignore rules for session artifacts (ticket 4817a5cc AC5)

use std::path::PathBuf;
use std::process::Command;

/// Verify that .gitignore correctly tracks durable artifacts and ignores local/ephemeral ones.
///
/// This test ensures AC1, AC2, and AC3 from ticket 4817a5cc remain enforced at the git level.
#[test]
fn gitignore_tracks_durable_ignores_local() {
    let repo_root = find_repo_root();

    // Test paths under .session/sessions/<id>/ (durable artifacts that should be tracked)
    let tracked_paths = vec![
        ".session/sessions/test-id/handoffs/test-handoff.json",
        ".session/sessions/test-id/finish.json",
        ".session/sessions/test-id/session.json",
        ".session/sessions/test-id/transcript.json",
        ".session/sessions/test-id/runs/run-123/transcript.json",
    ];

    // Test paths that should be ignored (local state, locks, logs, events)
    let ignored_paths = vec![
        ".session/local/test-pointer.json",
        ".session/sessions/test-id/test.lock",
        ".session/sessions/test-id/workspace.lock",
        ".session/sessions/test-id/session-capture-stop.log",
        ".session/sessions/test-id/events.json",
        ".session/sessions/test-id/runs/run-123/events.json",
    ];

    // Verify tracked paths are NOT ignored
    for path in &tracked_paths {
        let is_ignored = check_git_ignore(&repo_root, path);
        assert!(
            !is_ignored,
            "Path {} should be tracked (not ignored), but gitignore rule matched it",
            path
        );
    }

    // Verify local/ephemeral paths ARE ignored
    for path in &ignored_paths {
        let is_ignored = check_git_ignore(&repo_root, path);
        assert!(
            is_ignored,
            "Path {} should be ignored by gitignore, but no rule matched it",
            path
        );
    }
}

/// Check if a path is ignored by git using `git check-ignore`.
///
/// Returns true if the path is ignored, false if it would be tracked.
fn check_git_ignore(repo_root: &PathBuf, path: &str) -> bool {
    let output = Command::new("git")
        .arg("check-ignore")
        .arg("-q") // Quiet mode: exit code only
        .arg(path)
        .current_dir(repo_root)
        .output()
        .expect("Failed to run git check-ignore");

    // Exit code 0 means the path is ignored
    // Exit code 1 means the path is NOT ignored (would be tracked)
    output.status.code() == Some(0)
}

/// Find the repository root by walking up from the current directory.
fn find_repo_root() -> PathBuf {
    let mut current = std::env::current_dir().expect("Failed to get current directory");

    loop {
        if current.join(".git").exists() {
            return current;
        }

        if !current.pop() {
            panic!("Repository root not found (no .git directory in ancestor tree)");
        }
    }
}
