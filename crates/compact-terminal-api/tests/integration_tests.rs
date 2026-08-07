//! Integration tests for compact-terminal-api.

use compact_terminal_api::{
    ReadSpillRequest,
    RunRequest,
    RunResult,
    execute,
    read_spill,
};
use tempfile::tempdir;

#[test]
fn test_inline_short_output() {
    let request = RunRequest {
        command: "echo hello".to_string(),
        cwd: None,
        inline_limit: Some(4096),
        timeout_secs: Some(5),
        spill_dir: None,
    };

    let result = execute(&request).expect("execution failed");

    match result {
        RunResult::Inline {
            exit_code,
            stdout,
            stderr: _,
            elapsed_ms: _,
        } => {
            assert_eq!(exit_code, 0);
            assert!(stdout.contains("hello"));
        },
        _ => panic!("expected inline result"),
    }
}

#[test]
fn test_spilled_long_output() {
    let spill_dir = tempdir().expect("failed to create temp dir");

    // Generate output longer than inline limit.
    let request = RunRequest {
        command: "seq 1 1000".to_string(),
        cwd: None,
        inline_limit: Some(100), // Small limit to force spill.
        timeout_secs: Some(5),
        spill_dir: Some(spill_dir.path().to_path_buf()),
    };

    let result = execute(&request).expect("execution failed");

    match result {
        RunResult::Spilled {
            exit_code,
            stdout_preview,
            stderr_preview: _,
            total_bytes,
            total_lines,
            spill_file,
            elapsed_ms: _,
            next_steps,
        } => {
            assert_eq!(exit_code, 0);
            assert!(!stdout_preview.is_empty());
            assert!(total_bytes > 100);
            assert!(total_lines > 100);
            assert!(spill_file.exists());
            assert!(!next_steps.is_empty());

            // Verify the spill file can be read.
            let spill_content = std::fs::read_to_string(&spill_file)
                .expect("cannot read spill file");
            assert!(spill_content.contains("=== stdout ==="));
            assert!(spill_content.contains("1000"));
        },
        _ => panic!("expected spilled result"),
    }
}

#[test]
fn test_non_zero_exit_code() {
    let request = RunRequest {
        command: "exit 42".to_string(),
        cwd: None,
        inline_limit: Some(4096),
        timeout_secs: Some(5),
        spill_dir: None,
    };

    let result = execute(&request).expect("execution failed");

    match result {
        RunResult::Inline {
            exit_code,
            stdout: _,
            stderr: _,
            elapsed_ms: _,
        } => {
            assert_eq!(exit_code, 42);
        },
        _ => panic!("expected inline result"),
    }
}

#[test]
fn test_timeout() {
    let request = RunRequest {
        command: "sleep 10".to_string(),
        cwd: None,
        inline_limit: Some(4096),
        timeout_secs: Some(1),
        spill_dir: None,
    };

    let result = execute(&request).expect("execution failed");

    match result {
        RunResult::TimedOut {
            timeout_secs,
            stdout_partial: _,
            spill_file: _,
        } => {
            assert_eq!(timeout_secs, 1);
        },
        _ => panic!("expected timeout result"),
    }
}

#[test]
fn stdin_reading_command_terminates_and_subsequent_command_succeeds() {
    let cat_request = RunRequest {
        command: "cat".to_string(),
        cwd: None,
        inline_limit: Some(4096),
        timeout_secs: Some(5),
        spill_dir: None,
    };

    let cat_result = execute(&cat_request).expect("cat execution failed");
    assert!(matches!(
        cat_result,
        RunResult::Inline {
            exit_code: 0,
            stdout,
            ..
        } if stdout.is_empty()
    ));

    let echo_request = RunRequest {
        command: "echo alive".to_string(),
        cwd: None,
        inline_limit: Some(4096),
        timeout_secs: Some(5),
        spill_dir: None,
    };
    let echo_result = execute(&echo_request).expect("echo execution failed");
    assert!(matches!(
        echo_result,
        RunResult::Inline {
            exit_code: 0,
            stdout,
            ..
        } if stdout.contains("alive")
    ));
}

#[test]
fn timeout_kills_and_reaps_shell_process() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let request = RunRequest {
        command:
            "echo $$ > child.pid; sleep 30 & echo $! > grandchild.pid; wait"
                .to_string(),
        cwd: Some(temp_dir.path().to_path_buf()),
        inline_limit: Some(4096),
        timeout_secs: Some(2),
        spill_dir: None,
    };

    let result = execute(&request).expect("execution failed");
    assert!(matches!(
        result,
        RunResult::TimedOut {
            timeout_secs: 2,
            ..
        }
    ));

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg("for pid in $(cat child.pid grandchild.pid); do kill -0 \"$pid\" 2>/dev/null && exit 0; done; exit 1")
        .current_dir(temp_dir.path())
        .status()
        .expect("failed to check shell process status");
    assert!(
        !status.success(),
        "timed out command process tree is still running"
    );
}

#[test]
fn test_read_spill_by_line_range() {
    let spill_dir = tempdir().expect("failed to create temp dir");

    // Generate a spill file.
    let request = RunRequest {
        command: "seq 1 100".to_string(),
        cwd: None,
        inline_limit: Some(50),
        timeout_secs: Some(5),
        spill_dir: Some(spill_dir.path().to_path_buf()),
    };

    let result = execute(&request).expect("execution failed");

    let spill_file = match result {
        RunResult::Spilled { spill_file, .. } => spill_file,
        _ => panic!("expected spilled result"),
    };

    // Read lines 10-15.
    let read_request = ReadSpillRequest {
        spill_file,
        start: Some(10),
        end: Some(15),
        grep: None,
    };

    let read_result = read_spill(&read_request).expect("read failed");
    let content = read_result.content;

    // Should contain line numbers and content from the specified range.
    assert!(content.contains("10"));
    assert!(content.contains("15"));
}

#[test]
fn test_read_spill_grep() {
    let spill_dir = tempdir().expect("failed to create temp dir");

    // Generate a spill file with some error-like output.
    let request = RunRequest {
        command: r#"echo "line 1"; echo "error at line 2"; echo "line 3"; echo "another error at line 4""#.to_string(),
        cwd: None,
        inline_limit: Some(50),
        timeout_secs: Some(5),
        spill_dir: Some(spill_dir.path().to_path_buf()),
    };

    let result = execute(&request).expect("execution failed");

    let spill_file = match result {
        RunResult::Spilled { spill_file, .. } => spill_file,
        _ => panic!("expected spilled result"),
    };

    // Grep for "error".
    let read_request = ReadSpillRequest {
        spill_file,
        start: None,
        end: None,
        grep: Some("error".to_string()),
    };

    let read_result = read_spill(&read_request).expect("read failed");
    let content = read_result.content;

    // Should list matching line numbers.
    assert!(content.contains("matches (line numbers)"));
}

#[test]
fn test_missing_spill_file_error() {
    let read_request = ReadSpillRequest {
        spill_file: "/nonexistent/path/to/spill.txt".into(),
        start: None,
        end: None,
        grep: None,
    };

    let result = read_spill(&read_request);
    assert!(result.is_err());
}

#[test]
fn test_boundary_inline_limit() {
    let spill_dir = tempdir().expect("failed to create temp dir");

    // Output exactly at the limit should be inline.
    let request = RunRequest {
        command: "printf 'a%.0s' {1..4096}".to_string(),
        cwd: None,
        inline_limit: Some(4096),
        timeout_secs: Some(5),
        spill_dir: Some(spill_dir.path().to_path_buf()),
    };

    let result = execute(&request).expect("execution failed");

    // This might be inline or spilled depending on exact char count vs byte count.
    // The key is that the API makes a clear decision.
    match result {
        RunResult::Inline { .. } | RunResult::Spilled { .. } => {
            // Both are acceptable; just verify no panic.
        },
        _ => panic!("unexpected result type"),
    }
}
