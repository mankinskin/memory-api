use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ticket::TicketManifest;

pub const TICKET_MANIFEST_FILE: &str = "ticket.toml";
pub const TICKET_ASSETS_DIR: &str = "assets";
pub const TICKET_LOCK_FILE: &str = ".ticket-lock";
pub const TICKET_HISTORY_FILE: &str = "history.ndjson";
pub const TICKET_INTERVIEW_DIR: &str = "assets/interviews";
pub const TICKET_INTERVIEW_QUESTIONS_FILE: &str = "assets/interviews/questions.md";
pub const TICKET_INTERVIEW_ANSWERS_FILE: &str = "assets/interviews/answers.md";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanRoot {
    pub path: PathBuf,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketFolderContract {
    pub manifest_file: &'static str,
    pub assets_dir: &'static str,
    pub lock_file: &'static str,
}

impl Default for TicketFolderContract {
    fn default() -> Self {
        Self {
            manifest_file: TICKET_MANIFEST_FILE,
            assets_dir: TICKET_ASSETS_DIR,
            lock_file: TICKET_LOCK_FILE,
        }
    }
}

pub fn parse_ticket_manifest_toml(path: PathBuf, content: &str) -> Result<TicketManifest, ParseDiagnostic> {
    toml::from_str::<TicketManifest>(content).map_err(|err| ParseDiagnostic {
        path,
        reason: err.to_string(),
    })
}

pub fn has_minimum_ticket_contract(entries: &[&str]) -> bool {
    entries.contains(&TICKET_MANIFEST_FILE)
}
