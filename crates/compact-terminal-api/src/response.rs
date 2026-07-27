use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Result of a run command execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunResult {
    /// Short output returned inline.
    Inline {
        exit_code: i32,
        stdout: String,
        stderr: String,
        elapsed_ms: u128,
    },
    /// Long output spilled to a transient file.
    Spilled {
        exit_code: i32,
        /// First `inline_limit` bytes of stdout for quick scanning.
        stdout_preview: String,
        /// First `inline_limit` bytes of stderr.
        stderr_preview: String,
        /// Total bytes of combined output stored in the spill file.
        total_bytes: usize,
        /// Total lines in the spill file.
        total_lines: usize,
        /// Path to the transient file containing the full output.
        spill_file: PathBuf,
        elapsed_ms: u128,
        /// Suggested follow-up inspection commands.
        next_steps: Vec<String>,
    },
    /// Command timed out.
    TimedOut {
        timeout_secs: u64,
        /// Partial stdout captured before timeout.
        stdout_partial: String,
        spill_file: Option<PathBuf>,
    },
    /// Command could not be launched.
    LaunchError { message: String },
}

/// Result of reading a spill file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadSpillResult {
    pub content: String,
}
