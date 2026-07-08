use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{
        Command,
        Stdio,
    },
    time::{
        SystemTime,
        UNIX_EPOCH,
    },
};

use session_api::{
    SessionStoreConfig,
    copilot_payload_from_transcript_path,
};
use tempfile::tempdir;

const FIXTURE_SESSION_ID: &str = "fixture-local-a";
const LOCAL_FIXTURE_SESSION_ID: &str = "session-workspace-fixture";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("session-api crate should live under repo root")
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn find_cargo_bin() -> Option<String> {
    if let Ok(path) = std::env::var("CARGO") {
        if !path.trim().is_empty() {
            return Some(path);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(cargo_home) = std::env::var("CARGO_HOME") {
        candidates.push(
            PathBuf::from(cargo_home)
                .join("bin")
                .join(if cfg!(windows) { "cargo.exe" } else { "cargo" }),
        );
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        candidates.push(
            PathBuf::from(userprofile)
                .join(".cargo")
                .join("bin")
                .join("cargo.exe"),
        );
    }
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(
            PathBuf::from(home)
                .join(".cargo")
                .join("bin")
                .join(if cfg!(windows) { "cargo.exe" } else { "cargo" }),
        );
    }

    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    #[cfg(windows)]
    {
        if let Ok(output) = Command::new("where").arg("cargo").output() {
            if output.status.success() {
                if let Some(first) = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .find(|line| !line.trim().is_empty())
                {
                    return Some(first.trim().to_string());
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(output) = Command::new("which").arg("cargo").output() {
            if output.status.success() {
                if let Some(first) = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .find(|line| !line.trim().is_empty())
                {
                    return Some(first.trim().to_string());
                }
            }
        }
    }

    None
}

fn shell_single_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}

fn local_fixture_a() -> &'static str {
    include_str!("fixtures/local_parse_fixture_a.jsonl")
}

fn local_fixture_b() -> &'static str {
    include_str!("fixtures/local_parse_fixture_b.jsonl")
}

fn local_fixture_c() -> &'static str {
    include_str!("fixtures/local_parse_fixture_c.jsonl")
}

fn write_fixture_transcript(
    dir: &std::path::Path,
    name: &str,
    content: &str,
) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).expect("write local deterministic fixture transcript");
    path
}

#[test]
fn e2e_parses_fixture_transcript_payload() {
    let fixture_dir = tempdir().expect("temp fixture dir");
    let transcript_path =
        write_fixture_transcript(fixture_dir.path(), "fixture-a.jsonl", local_fixture_a());

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
    let fixture_dir = tempdir().expect("temp fixture dir");
    let transcript_path =
        write_fixture_transcript(fixture_dir.path(), "fixture-a.jsonl", local_fixture_a());

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
fn e2e_stop_hook_script_persists_fixture_from_nested_workspace_cwd() {
    let repo_root = repo_root();
    let script_source = repo_root.join("tools/agent-hooks/session-capture-stop.sh");
    assert!(
        script_source.is_file(),
        "missing hook script under repo root"
    );

    let fixture_text = include_str!("fixtures/stop_hook_workspace_e2e.jsonl");
    let suffix = unique_suffix();
    let workspace_fixture =
        tempdir().expect("create isolated workspace fixture tempdir");
    let fixture_root = workspace_fixture.path().join("workspace-fixture");
    let fixture_tools = fixture_root.join("tools").join("agent-hooks");
    let fixture_transcripts = fixture_root.join("transcripts");
    let fixture_store_root = fixture_root.join("session-store");
    fs::create_dir_all(&fixture_tools).expect("create fixture hook directory");
    fs::create_dir_all(&fixture_transcripts)
        .expect("create fixture transcript directory");
    fs::create_dir_all(&fixture_store_root)
        .expect("create fixture session store root");

    let fixture_script_path = fixture_tools.join("session-capture-stop.sh");
    fs::copy(&script_source, &fixture_script_path)
        .expect("copy hook script into fixture workspace");

    let rel_transcript_path = PathBuf::from("transcripts").join("copilot.jsonl");
    let abs_transcript_path = fixture_root.join(&rel_transcript_path);

    let workspace_slug = format!("fixture-workspace-{suffix}");
    let session_id = format!("{LOCAL_FIXTURE_SESSION_ID}-{suffix}");

    let transcript_text = fixture_text.replace(LOCAL_FIXTURE_SESSION_ID, &session_id);
    fs::write(&abs_transcript_path, transcript_text)
        .expect("write transcript fixture");

    let payload = serde_json::json!({
        "transcript_path": rel_transcript_path,
        "workspace_slug": &workspace_slug,
        "hook_event_name": "UserPromptSubmit",
        "session_id": &session_id,
    })
    .to_string();

    let Some(cargo_bin) = find_cargo_bin() else {
        eprintln!("skipping e2e shell-hook test: unable to locate cargo binary for bash subprocess");
        return;
    };

    let manifest_path = repo_root
        .join("memory-api/crates/session-api/Cargo.toml")
        .to_string_lossy()
        .replace("\\\\?\\", "")
        .replace('\\', "/");
    let script_path_shell = PathBuf::from("tools/agent-hooks/session-capture-stop.sh")
        .to_string_lossy()
        .to_string();
    let command_line = format!(
        "SESSION_CAPTURE_STORE_ROOT={} SESSION_CAPTURE_MANIFEST_PATH={} SESSION_CAPTURE_CARGO_BIN={} bash {}",
        shell_single_quote("session-store"),
        shell_single_quote(&manifest_path),
        shell_single_quote(&cargo_bin),
        shell_single_quote(&script_path_shell)
    );

    let mut command = Command::new("bash");
    command
        .arg("-lc")
        .arg(command_line)
        .current_dir(&fixture_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping e2e shell-hook test: bash not available on PATH");
            return;
        },
        Err(error) => panic!("failed to spawn bash for hook test: {error}"),
    };

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(payload.as_bytes())
        .expect("write hook payload to stdin");

    let output = child.wait_with_output().expect("wait for hook process");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() && stderr.contains("cargo binary not found") {
        eprintln!("skipping e2e shell-hook test: bash subprocess could not resolve cargo binary");
        return;
    }

    assert!(
        output.status.success(),
        "session-capture-stop.sh failed: stdout={stdout} stderr={stderr}"
    );
    assert_eq!(stdout.trim(), "{}", "stop hook should emit empty JSON sentinel");
    assert!(
        !stderr.contains("skip: transcript not found"),
        "hook skipped transcript unexpectedly: stdout={stdout} stderr={stderr}"
    );

    let session_manifest = fixture_store_root
        .join("sessions")
        .join(&session_id)
        .join("session.json");
    assert!(
        session_manifest.is_file(),
        "session manifest missing at {} (stdout={} stderr={})",
        session_manifest.display(),
        stdout,
        stderr
    );

    let leaked_root_manifest = repo_root
        .join(".session")
        .join("sessions")
        .join(&session_id)
        .join("session.json");
    assert!(
        !leaked_root_manifest.is_file(),
        "hook leaked test artifact into root store: {}",
        leaked_root_manifest.display()
    );

    let config = SessionStoreConfig::new(&fixture_store_root, &workspace_slug);
    let record = config
        .read_session(&session_id)
        .expect("stop hook should persist fixture transcript into the temp store");

    assert_eq!(record.session_id, session_id);
    assert_eq!(record.metadata.workspace_slug, workspace_slug);
    assert_eq!(record.metadata.trigger.as_deref(), Some("UserPromptSubmit"));
    assert_eq!(record.turns.len(), 2);
    assert_eq!(record.turns[0].content, "Persist this transcript from fixture");
    assert_eq!(record.turns[1].content, "Transcript persisted from fixture.");

    let session_dir = fixture_store_root
        .join("sessions")
        .join(&session_id);
    assert!(session_dir.join("session.json").is_file());
    assert!(session_dir.join("transcript.json").is_file());
    assert!(session_dir.join("events.json").is_file());
}

#[test]
fn e2e_parses_multiple_local_fixture_scenarios() {
    let fixture_dir = tempdir().expect("temp fixture dir");
    let fixtures = [
        ("fixture-a.jsonl", local_fixture_a(), "fixture-local-a"),
        ("fixture-b.jsonl", local_fixture_b(), "fixture-local-b"),
        ("fixture-c.jsonl", local_fixture_c(), "fixture-local-c"),
    ];

    for (name, content, expected_session_id) in fixtures {
        let path = write_fixture_transcript(fixture_dir.path(), name, content);
        let payload = copilot_payload_from_transcript_path(
            &path,
            "default",
            Some("e2e-scan".to_string()),
        )
        .expect("local deterministic fixture transcript should parse");

        assert_eq!(payload.session_id, expected_session_id);
        assert!(
            !payload.messages.is_empty(),
            "expected visible messages for fixture {name}"
        );
    }
}
