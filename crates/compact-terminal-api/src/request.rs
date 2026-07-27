use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Request to run a shell command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRequest {
    /// The shell command to execute (passed to `sh -c`).
    pub command: String,

    /// Working directory for the command. Defaults to the current working dir.
    pub cwd: Option<PathBuf>,

    /// Maximum bytes to return inline. Outputs exceeding this are spilled to a
    /// transient file and summarised. Default: 4096.
    pub inline_limit: Option<usize>,

    /// Command timeout in seconds. Default: 60.
    pub timeout_secs: Option<u64>,

    /// Directory where spill files will be written. Defaults to system temp dir.
    pub spill_dir: Option<PathBuf>,
}

/// Request to read from a spill file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadSpillRequest {
    /// Path to the transient spill file returned by a previous `run` call.
    pub spill_file: PathBuf,

    /// First line to read (1-based, inclusive). Defaults to 1.
    pub start: Option<usize>,

    /// Last line to read (1-based, inclusive). Defaults to start + 80.
    pub end: Option<usize>,

    /// Search pattern: returns matching line numbers (1-based) instead of content.
    pub grep: Option<String>,
}
