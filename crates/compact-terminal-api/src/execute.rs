use std::{
    env,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

use uuid::Uuid;

use crate::{
    error::CompactTerminalError,
    request::RunRequest,
    response::RunResult,
};

/// Default maximum bytes to return inline. Outputs longer than this are spilled to file.
pub const DEFAULT_INLINE_LIMIT: usize = 4096;

/// Default command timeout in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Execute a shell command and return the result.
///
/// Short outputs (≤ inline_limit bytes) are returned directly.
/// Long outputs are summarised inline and stored in a transient file.
pub fn execute(request: &RunRequest) -> Result<RunResult, CompactTerminalError> {
    let inline_limit = request.inline_limit.unwrap_or(DEFAULT_INLINE_LIMIT);
    let timeout_secs = request.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    let spill_dir = request
        .spill_dir
        .clone()
        .unwrap_or_else(|| env::temp_dir().join("compact-terminal-api"));

    let start = std::time::Instant::now();

    // Build the command.
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(&request.command);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(ref cwd) = request.cwd {
        cmd.current_dir(cwd);
    }

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(RunResult::LaunchError {
                message: format!("failed to spawn '{}': {e}", request.command),
            });
        },
    };

    // Simple timeout implementation using threads.
    // We spawn a thread to wait for the child, and the main thread waits with timeout.
    let timeout_duration = Duration::from_secs(timeout_secs);

    // Use a channel to communicate the result.
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(timeout_duration) {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            return Ok(RunResult::LaunchError {
                message: format!("command failed: {e}"),
            });
        },
        Err(_timeout) => {
            return Ok(RunResult::TimedOut {
                timeout_secs,
                stdout_partial: String::new(),
                spill_file: None,
            });
        },
    };

    let elapsed_ms = start.elapsed().as_millis();
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let combined_len = stdout.len() + stderr.len();

    if combined_len <= inline_limit {
        // Short output — return inline.
        return Ok(RunResult::Inline {
            exit_code,
            stdout,
            stderr,
            elapsed_ms,
        });
    }

    // Long output — spill to file.
    let spill_content = format!(
        "=== stdout ===\n{stdout}\n=== stderr ===\n{stderr}\n=== exit_code: {exit_code} ===\n"
    );
    let total_bytes = spill_content.len();
    let total_lines = spill_content.lines().count();

    let spill_file = match write_spill(&spill_dir, &spill_content) {
        Ok(p) => p,
        Err(e) => {
            // Cannot spill — return error.
            return Ok(RunResult::LaunchError {
                message: format!(
                    "output too large ({combined_len} bytes) and spill failed: {e}"
                ),
            });
        },
    };

    let stdout_preview = stdout
        .chars()
        .take(inline_limit / 2)
        .collect::<String>();
    let stderr_preview = stderr
        .chars()
        .take(inline_limit / 2)
        .collect::<String>();

    let spill_str = spill_file.display().to_string();
    let next_steps = vec![
        format!("peek \"{spill_str}\" --count"),
        format!("peek \"{spill_str}\" --grep \"error\" --window 10"),
        format!("peek \"{spill_str}\" --head 30"),
        format!(
            "Use read_spill with start/end or grep to inspect targeted sections"
        ),
    ];

    Ok(RunResult::Spilled {
        exit_code,
        stdout_preview,
        stderr_preview,
        total_bytes,
        total_lines,
        spill_file,
        elapsed_ms,
        next_steps,
    })
}

fn write_spill(
    spill_dir: &PathBuf,
    content: &str,
) -> Result<PathBuf, CompactTerminalError> {
    std::fs::create_dir_all(spill_dir).map_err(|source| {
        CompactTerminalError::CannotCreateSpillDir {
            path: spill_dir.clone(),
            source,
        }
    })?;
    let path = spill_dir.join(format!("{}.txt", Uuid::new_v4()));
    let mut f = std::fs::File::create(&path).map_err(|source| {
        CompactTerminalError::CannotWriteSpillFile {
            path: path.clone(),
            source,
        }
    })?;
    f.write_all(content.as_bytes()).map_err(|source| {
        CompactTerminalError::CannotWriteSpillFile {
            path: path.clone(),
            source,
        }
    })?;
    Ok(path)
}
