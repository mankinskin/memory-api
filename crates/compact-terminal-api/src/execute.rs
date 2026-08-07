use std::{
    env,
    io::{
        Read,
        Write,
    },
    path::PathBuf,
    process::{
        Command,
        Stdio,
    },
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
pub fn execute(
    request: &RunRequest
) -> Result<RunResult, CompactTerminalError> {
    let inline_limit = request.inline_limit.unwrap_or(DEFAULT_INLINE_LIMIT);
    let timeout_secs = request.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    let spill_dir = request
        .spill_dir
        .clone()
        .unwrap_or_else(|| env::temp_dir().join("compact-terminal-api"));

    let start = std::time::Instant::now();

    // Build the command.
    let mut cmd = Command::new("sh");
    #[cfg(windows)]
    let pid_file = env::temp_dir()
        .join(format!("compact-terminal-{}.pid", Uuid::new_v4()));
    #[cfg(windows)]
    {
        cmd.arg("-c")
            .arg("printf '%s' \"$$\" > \"$COMPACT_TERMINAL_PID_FILE\"; eval \"$1\"")
            .arg("sh")
            .arg(&request.command)
            .env("COMPACT_TERMINAL_PID_FILE", &pid_file);
    }
    #[cfg(not(windows))]
    cmd.arg("-c").arg(&request.command);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(ref cwd) = request.cwd {
        cmd.current_dir(cwd);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(RunResult::LaunchError {
                message: format!("failed to spawn '{}': {e}", request.command),
            });
        },
    };

    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (output_tx, output_rx) = std::sync::mpsc::channel();
    let stdout_tx = output_tx.clone();
    let stdout_reader = std::thread::spawn(move || {
        read_output(&mut stdout, true, stdout_tx);
    });
    let stderr_reader = std::thread::spawn(move || {
        read_output(&mut stderr, false, output_tx);
    });

    let timeout_duration = Duration::from_secs(timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout_duration => {
                std::thread::sleep(Duration::from_millis(10));
            },
            Ok(None) => {
                #[cfg(windows)]
                let kill_result = kill_child_tree(&mut child, &pid_file);
                #[cfg(not(windows))]
                let kill_result = kill_child_tree(&mut child);
                if let Err(error) = kill_result {
                    return Ok(RunResult::LaunchError {
                        message: format!(
                            "failed to kill timed out command: {error}"
                        ),
                    });
                }
                if let Err(error) = child.wait() {
                    return Ok(RunResult::LaunchError {
                        message: format!(
                            "failed to reap timed out command: {error}"
                        ),
                    });
                }
                #[cfg(windows)]
                let _ = std::fs::remove_file(&pid_file);
                let (stdout, _stderr) = collect_available_output(&output_rx);
                drop(stdout_reader);
                drop(stderr_reader);
                return Ok(RunResult::TimedOut {
                    timeout_secs,
                    stdout_partial: String::from_utf8_lossy(&stdout)
                        .into_owned(),
                    spill_file: None,
                });
            },
            Err(error) => {
                return Ok(RunResult::LaunchError {
                    message: format!("failed to wait for command: {error}"),
                });
            },
        }
    };

    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    let (stdout, stderr) = collect_available_output(&output_rx);
    #[cfg(windows)]
    let _ = std::fs::remove_file(&pid_file);

    let elapsed_ms = start.elapsed().as_millis();
    let exit_code = status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    let stderr = String::from_utf8_lossy(&stderr).into_owned();

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

    let stdout_preview =
        stdout.chars().take(inline_limit / 2).collect::<String>();
    let stderr_preview =
        stderr.chars().take(inline_limit / 2).collect::<String>();

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

fn read_output(
    reader: &mut impl Read,
    is_stdout: bool,
    sender: std::sync::mpsc::Sender<(bool, Vec<u8>)>,
) {
    let mut buffer = [0; 8192];
    loop {
        let Ok(bytes_read) = reader.read(&mut buffer) else {
            return;
        };
        if bytes_read == 0
            || sender
                .send((is_stdout, buffer[..bytes_read].to_vec()))
                .is_err()
        {
            return;
        }
    }
}

fn collect_available_output(
    receiver: &std::sync::mpsc::Receiver<(bool, Vec<u8>)>
) -> (Vec<u8>, Vec<u8>) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    while let Ok((is_stdout, bytes)) = receiver.try_recv() {
        if is_stdout {
            stdout.extend(bytes);
        } else {
            stderr.extend(bytes);
        }
    }
    (stdout, stderr)
}

#[cfg(windows)]
fn kill_child_tree(
    child: &mut std::process::Child,
    pid_file: &std::path::Path,
) -> std::io::Result<()> {
    let pid = std::fs::read_to_string(pid_file)?;
    let status = Command::new("sh")
        .args([
            "-c",
            "children() { ps -ef | awk -v parent=\"$1\" '$3 == parent { print $2 }'; }; kill_tree() { for descendant in $(children \"$1\"); do kill_tree \"$descendant\"; done; kill -KILL \"$1\" 2>/dev/null; }; kill_tree \"$1\"",
            "sh",
            &pid,
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        child.kill()
    }
}

#[cfg(not(windows))]
fn kill_child_tree(child: &mut std::process::Child) -> std::io::Result<()> {
    child.kill()
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
