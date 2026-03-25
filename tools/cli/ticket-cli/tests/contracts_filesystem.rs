use chrono::Utc;
use ticket_api::model::filesystem::{
    TICKET_ASSETS_DIR, TICKET_INTERVIEW_ANSWERS_FILE, TICKET_INTERVIEW_QUESTIONS_FILE,
    TICKET_LOCK_FILE, TICKET_MANIFEST_FILE, TicketFolderContract,
    has_minimum_ticket_contract, parse_ticket_manifest_toml,
};
use ticket_api::model::ticket::TicketManifest;
use std::path::PathBuf;
use uuid::Uuid;

#[test]
fn folder_contract_defaults_are_stable() {
    let contract = TicketFolderContract::default();

    assert_eq!(contract.manifest_file, TICKET_MANIFEST_FILE);
    assert_eq!(contract.assets_dir, TICKET_ASSETS_DIR);
    assert_eq!(contract.lock_file, TICKET_LOCK_FILE);
}

#[test]
fn minimum_contract_requires_ticket_toml() {
    assert!(has_minimum_ticket_contract(&["ticket.toml", "description.md"]));
    assert!(!has_minimum_ticket_contract(&["description.md", "assets"]));
}

#[test]
fn interview_contract_paths_are_stable() {
    assert_eq!(TICKET_INTERVIEW_QUESTIONS_FILE, "assets/interviews/questions.md");
    assert_eq!(TICKET_INTERVIEW_ANSWERS_FILE, "assets/interviews/answers.md");
}

#[test]
fn valid_manifest_is_parsed_for_discovered_folder() {
    let manifest = TicketManifest::new(Uuid::new_v4(), Utc::now());
    let content = toml::to_string(&manifest).expect("manifest encodes");

    let parsed = parse_ticket_manifest_toml(PathBuf::from("/tmp/ticket.toml"), &content)
        .expect("valid manifest parses");

    assert_eq!(parsed.id, manifest.id);
    assert_eq!(parsed.created_at, manifest.created_at);
}

#[test]
fn broken_manifest_returns_path_and_reason() {
    let path = PathBuf::from("/tmp/ticket.toml");
    let bad = "id = \"broken\"\ncreated_at = \"nope\"\n";

    let diag = parse_ticket_manifest_toml(path.clone(), bad).expect_err("broken manifest must fail");

    assert_eq!(diag.path, path);
    assert!(!diag.reason.is_empty());
}
