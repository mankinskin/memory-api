use std::{
    fs,
    path::PathBuf,
    process::Command,
};

use session_api::{
    copilot_payload_from_transcript_path,
    SessionStoreConfig,
};
use tempfile::tempdir;

const DEFAULT_TRANSCRIPTS_ROOT: &str =
    "C:/Users/linus/AppData/Roaming/Code/User/workspaceStorage/85c65471aaff0b651db0ce38f3719fa7/GitHub.copilot-chat/transcripts";
const FIXTURE_SESSION_ID: &str = "38095e95-c056-478a-8fe4-2b0a80f34573";

fn transcripts_root() -> PathBuf {
    std::env::var("COPILOT_TRANSCRIPTS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_TRANSCRIPTS_ROOT))
}

fn fixture_transcript_path() -> PathBuf {
    transcripts_root().join(format!("{FIXTURE_SESSION_ID}.jsonl"))
}

fn require_fixture(path: &PathBuf) -> bool {
    if path.is_file() {
        return true;
    }

    eprintln!(
        "skipping e2e fixture-dependent test: transcript not found at {}",
        path.display()
    );
    false
}

#[test]
fn e2e_parses_fixture_transcript_payload() {
    let transcript_path = fixture_transcript_path();
    if !require_fixture(&transcript_path) {
        return;
    }

    let payload = copilot_payload_from_transcript_path(
        &transcript_path,
        "default",
        Some("e2e-parse".to_string()),
    )
    .expect("fixture transcript should parse into payload");

    assert_eq!(payload.session_id, FIXTURE_SESSION_ID);
    assert!(!payload.messages.is_empty());
}

#[test]
fn e2e_hook_binary_persists_fixture_transcript() {
    let transcript_path = fixture_transcript_path();
    if !require_fixture(&transcript_path) {
        return;
    }

    let store_dir = tempdir().expect("tempdir");
    let store_root = store_dir.path().join("memory-api-store");
    fs::create_dir_all(&store_root).expect("create temp store root");

    let hook_bin = std::env::var("CARGO_BIN_EXE_copilot-stop-hook")
        .expect("cargo should expose copilot-stop-hook binary path for integration tests");

    let output = Command::new(hook_bin)
        .arg("--transcript-path")
        .arg(&transcript_path)
        .arg("--store-root")
        .arg(&store_root)
        .arg("--workspace-slug")
        .arg("default")
        .arg("--trigger")
        .arg("UserPromptSubmit")
        .output()
        .expect("run copilot-stop-hook");

    assert!(
        output.status.success(),
        "copilot-stop-hook failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config = SessionStoreConfig::new(&store_root, "default");
    let record = config
        .read_session(FIXTURE_SESSION_ID)
        .expect("persisted session should be readable from temp store");

    assert!(!record.turns.is_empty());
    assert_eq!(record.session_id, FIXTURE_SESSION_ID);
    assert_eq!(record.metadata.workspace_slug, "default");
    assert_eq!(record.metadata.trigger.as_deref(), Some("UserPromptSubmit"));
}

#[test]
fn e2e_parses_multiple_transcript_fixtures_from_root() {
    let root = transcripts_root();
    if !root.is_dir() {
        eprintln!(
            "skipping root fixture scan: transcript root does not exist at {}",
            root.display()
        );
        return;
    }

    let mut fixtures = fs::read_dir(&root)
        .expect("read transcript fixture root")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();
    fixtures.sort();

    let sample_size = fixtures.len().min(3);
    assert!(sample_size > 0, "expected at least one .jsonl transcript fixture");

    let mut parsed = 0usize;
    for path in fixtures.iter().take(sample_size) {
        if let Ok(payload) =
            copilot_payload_from_transcript_path(path, "default", Some("e2e-scan".to_string()))
        {
            if !payload.messages.is_empty() {
                parsed += 1;
            }
        }
    }

    assert!(
        parsed > 0,
        "expected at least one sampled fixture transcript to parse with visible messages"
    );
}
