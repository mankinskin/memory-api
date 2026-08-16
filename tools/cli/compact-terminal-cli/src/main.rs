//! compact-terminal — token-bounded terminal output utility
//!
//! Executes shell commands with bounded output. Short results are returned directly,
//! long results are truncated inline and spilled to a file for targeted follow-up.
//!
//! ## Usage
//!
//! ```text
//! # Run command with default inline limit (4096 bytes)
//! compact-terminal run "cargo test"
//!
//! # Run with custom inline limit (2048 bytes)
//! compact-terminal run "cargo build" --inline-limit 2048
//!
//! # Run with custom timeout (120 seconds)
//! compact-terminal run "cargo test" --timeout 120
//!
//! # Run from specific working directory
//! compact-terminal run "npm test" --cwd /path/to/project
//!
//! # Read from a spill file (lines 1-50)
//! compact-terminal read-spill /tmp/compact-terminal-mcp/abc123.txt --start 1 --end 50
//!
//! # Search in a spill file
//! compact-terminal read-spill /tmp/compact-terminal-mcp/abc123.txt --grep "error"
//! ```

use std::path::PathBuf;

use clap::{
    Parser,
    Subcommand,
};
use compact_terminal_api::{
    CompactTerminalError,
    ReadSpillRequest,
    RunRequest,
    execute,
    read_spill,
};

/// compact-terminal — token-bounded terminal output.
///
/// Executes shell commands with bounded inline output. Long outputs are truncated
/// and spilled to a transient file for targeted follow-up inspection.
#[derive(Debug, Parser)]
#[command(name = "compact-terminal", version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a shell command with bounded inline output.
    ///
    /// Short outputs (≤ inline_limit bytes) are returned directly.
    /// Long outputs are summarized inline and stored in a transient file.
    ///
    /// The output mode is **bounded by default** — the inline_limit caps token
    /// consumption. For full unbounded output, set a very high inline_limit.
    Run {
        /// The shell command to execute.
        command: String,

        /// Working directory for the command. Defaults to current directory.
        #[arg(long)]
        cwd: Option<PathBuf>,

        /// Maximum bytes to return inline (bounded mode, default: 4096).
        ///
        /// Outputs exceeding this are spilled to a transient file and summarized.
        /// Set a very high value for unbounded output (not recommended).
        #[arg(long, default_value = "4096")]
        inline_limit: usize,

        /// Command timeout in seconds (default: 60).
        #[arg(long, default_value = "60")]
        timeout: u64,

        /// Directory for spill files. Defaults to system temp dir.
        #[arg(long)]
        spill_dir: Option<PathBuf>,
    },

    /// Read from a transient spill file returned by a previous run.
    ///
    /// Use this for targeted follow-up inspection instead of re-running
    /// the full command. Supports line ranges and grep patterns.
    ReadSpill {
        /// Path to the transient spill file.
        spill_file: PathBuf,

        /// First line to read (1-based, inclusive). Defaults to 1.
        #[arg(long, default_value = "1")]
        start: usize,

        /// Last line to read (1-based, inclusive). Defaults to start + 80.
        #[arg(long)]
        end: Option<usize>,

        /// Search pattern: returns matching line numbers instead of content.
        #[arg(long)]
        grep: Option<String>,
    },
}

fn main() -> Result<(), CompactTerminalError> {
    let args = Args::parse();

    match args.command {
        Command::Run {
            command,
            cwd,
            inline_limit,
            timeout,
            spill_dir,
        } => {
            let request = RunRequest {
                command,
                cwd,
                inline_limit: Some(inline_limit),
                timeout_secs: Some(timeout),
                spill_dir,
            };

            let result = execute(&request)?;

            // Print result as JSON for machine consumption
            let json = serde_json::to_string_pretty(&result).map_err(|e| {
                CompactTerminalError::InvalidRequest(format!(
                    "serialization error: {}",
                    e
                ))
            })?;
            println!("{}", json);

            Ok(())
        },

        Command::ReadSpill {
            spill_file,
            start,
            end,
            grep,
        } => {
            let request = ReadSpillRequest {
                spill_file,
                start: Some(start),
                end,
                grep,
            };

            let result = read_spill(&request)?;

            // Print content directly
            print!("{}", result.content);

            Ok(())
        },
    }
}
