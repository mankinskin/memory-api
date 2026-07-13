use std::{
    env,
    io::Read,
    path::{
        Path,
        PathBuf,
    },
    process,
};

use serde_json::Value;

use session_api::{
    FeedbackSignalKind,
    FollowUpSynthesisOutcome,
    SessionError,
    SessionStoreConfig,
    SessionStorePlan,
    build_follow_up_ticket_draft,
    mine_explicit_ingestion_signals,
    mine_failed_tool_call_signals,
    mine_structured_feedback_signals,
    synthesize_follow_up_ticket,
};
use ticket_api::storage::TicketStore;

fn main() {
    match run() {
        Ok(()) => {},
        Err(SessionError::InvalidHookInput(message)) if message == "help" => {
            print_usage();
        },
        Err(error) => {
            eprintln!("[copilot-capture-hook] {error}");
            process::exit(1);
        },
    }
}

fn run() -> Result<(), SessionError> {
    let args = parse_args()?;
    let args = if args.from_hook_stdin {
        args_from_hook_stdin(args)?
    } else {
        args
    };

    let transcript_path = normalize_transcript_path(&args.transcript_path);
    if !transcript_path.is_file() {
        eprintln!(
            "[copilot-capture-hook] skip: transcript not found at {}",
            transcript_path.display()
        );
        println!("{{}}");
        return Ok(());
    }

    let store_root = resolve_store_root(
        args.store_root,
        memory_api::workspace::working_dir().as_deref(),
    );
    let config =
        SessionStoreConfig::new(store_root.clone(), args.workspace_slug);

    let plan = config.capture_copilot_transcript(transcript_path, args.trigger)?;
    report_structured_feedback_signals(&plan);
    synthesize_follow_up_tickets(
        &plan,
        memory_api::workspace::working_dir().as_deref(),
    );
    println!("{{}}");
    Ok(())
}

/// Detect structured feedback signals in the just-captured session and log a
/// compact summary for observability.
///
/// This intentionally does **not** create tickets or write feedback entries.
/// The previous implementation mined free-text with a keyword / confusion-marker
/// heuristic and auto-created tracker tickets, which produced large volumes of
/// false positives (over a hundred spurious tickets in a single run). Auto
/// synthesis is paused until (1) signals are derived only from structured
/// metadata and (2) a backtraceable, verifiable ticket format is defined.
///
/// Two failed-tool-call miners run: the turn-based
/// [`mine_structured_feedback_signals`] (kept for forward compatibility, in
/// case a future capture path populates `role: tool` turns) and the
/// event-based [`mine_failed_tool_call_signals`], which is what actually
/// fires against real captured transcripts — every committed session has
/// zero `role: tool` turns, so tool call/result metadata lives only in the
/// captured events list. The event-based miner also resolves each failure's
/// [`session_api::FailedToolCallMapping`] per the grounded policy.
///
/// `ExplicitIngestion` signals (captured `feedback_ingest` tool calls) are
/// also summarized here for observability, but are never auto-recorded: a
/// successful live call already persisted its own `FeedbackEntry`, and a
/// failed one is left for a dedicated recovery entry point
/// (`recover_feedback_entry_from_signal`) to avoid silently double-writing
/// or partially-writing feedback from a stop-hook code path.
fn report_structured_feedback_signals(plan: &SessionStorePlan) {
    let turn_signals = if plan.record.turns.is_empty() {
        Vec::new()
    } else {
        mine_structured_feedback_signals(&plan.record.turns)
    };
    let workspace_slug = plan.record.metadata.workspace_slug.as_str();
    let event_failed_tool_calls = plan
        .events
        .as_ref()
        .map(|events| {
            mine_failed_tool_call_signals(&events.events, workspace_slug)
        })
        .unwrap_or_default();
    let event_ingestions = plan
        .events
        .as_ref()
        .map(|events| mine_explicit_ingestion_signals(&events.events))
        .unwrap_or_default();

    if turn_signals.is_empty()
        && event_failed_tool_calls.is_empty()
        && event_ingestions.is_empty()
    {
        return;
    }

    let failed_tool_calls = turn_signals
        .iter()
        .chain(event_failed_tool_calls.iter())
        .filter(|signal| {
            matches!(signal.kind, FeedbackSignalKind::FailedToolCall)
        })
        .count();
    let explicit_ingestions = event_ingestions
        .iter()
        .filter(|signal| {
            matches!(signal.kind, FeedbackSignalKind::ExplicitIngestion)
        })
        .count();

    let signals: Vec<_> = turn_signals
        .iter()
        .chain(event_failed_tool_calls.iter())
        .chain(event_ingestions.iter())
        .collect();

    match serde_json::to_string(&signals) {
        Ok(json) => eprintln!(
            "[copilot-capture-hook] structured feedback signals for session {}: {} total ({} failed tool calls, {} explicit ingestions) {}",
            plan.record.session_id, signals.len(), failed_tool_calls, explicit_ingestions, json
        ),
        Err(error) => eprintln!(
            "[copilot-capture-hook] structured feedback signals for session {}: {} total ({} failed tool calls, {} explicit ingestions); summary serialization failed: {error}",
            plan.record.session_id, signals.len(), failed_tool_calls, explicit_ingestions
        ),
    }
}

/// Re-enable backtraceable, verifiable follow-up ticket synthesis, gated on
/// confident `ExplicitIngestion` signals only (see `session_api::follow_up`
/// module docs for the gating rationale and the idempotent-dedupe design).
/// Ticket-store errors are logged and skipped rather than failing the hook:
/// session capture must still succeed even if the ticket store is
/// unavailable.
fn synthesize_follow_up_tickets(
    plan: &SessionStorePlan,
    cwd: Option<&Path>,
) {
    let Some(events) = plan.events.as_ref() else {
        return;
    };
    let ingestion_signals = mine_explicit_ingestion_signals(&events.events);
    if ingestion_signals.is_empty() {
        return;
    }

    let ticket_root = match cwd {
        Some(cwd) =>
            memory_api::workspace::resolve_local_root_from(cwd, ".ticket"),
        None => PathBuf::from(".ticket"),
    };
    let ticket_store = match TicketStore::open_or_init(&ticket_root) {
        Ok(store) => store,
        Err(error) => {
            eprintln!(
                "[copilot-capture-hook] follow-up synthesis skipped: failed to open ticket store at {}: {error}",
                ticket_root.display()
            );
            return;
        },
    };

    for signal in &ingestion_signals {
        let draft = match build_follow_up_ticket_draft(
            signal,
            &plan.record.session_id,
        ) {
            Ok(Some(draft)) => draft,
            Ok(None) => continue,
            Err(error) => {
                eprintln!(
                    "[copilot-capture-hook] follow-up draft build failed for session {}: {error}",
                    plan.record.session_id
                );
                continue;
            },
        };

        match synthesize_follow_up_ticket(&ticket_store, &draft, None) {
            Ok(FollowUpSynthesisOutcome::Created(id)) => eprintln!(
                "[copilot-capture-hook] synthesized follow-up ticket {id} ({})",
                draft.dedupe_key
            ),
            Ok(FollowUpSynthesisOutcome::AlreadyExists(id)) => eprintln!(
                "[copilot-capture-hook] follow-up ticket {id} already exists for {} (no duplicate created)",
                draft.dedupe_key
            ),
            Err(error) => eprintln!(
                "[copilot-capture-hook] follow-up ticket synthesis failed for {}: {error}",
                draft.dedupe_key
            ),
        }
    }
}

fn resolve_store_root(
    store_root: Option<PathBuf>,
    cwd: Option<&Path>,
) -> PathBuf {
    match store_root {
        Some(store_root) => store_root,
        None => match cwd {
            Some(cwd) =>
                memory_api::workspace::resolve_local_root_from(cwd, ".session"),
            None => std::path::PathBuf::from(".session"),
        },
    }
}

struct Args {
    transcript_path: PathBuf,
    store_root: Option<PathBuf>,
    workspace_slug: String,
    trigger: String,
    from_hook_stdin: bool,
}

fn parse_args() -> Result<Args, SessionError> {
    let mut transcript_path = None;
    let mut store_root = None;
    let mut workspace_slug = Some("default".to_string());
    let mut trigger = Some("stop".to_string());
    let mut from_hook_stdin = false;

    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" =>
                return Err(SessionError::InvalidHookInput("help".to_string())),
            "--transcript-path" => {
                transcript_path = Some(PathBuf::from(next_value(
                    &mut arguments,
                    "--transcript-path",
                )?));
            },
            "--store-root" => {
                store_root = Some(PathBuf::from(next_value(
                    &mut arguments,
                    "--store-root",
                )?));
            },
            "--workspace-slug" => {
                workspace_slug =
                    Some(next_value(&mut arguments, "--workspace-slug")?);
            },
            "--trigger" => {
                trigger = Some(next_value(&mut arguments, "--trigger")?);
            },
            "--from-hook-stdin" => {
                from_hook_stdin = true;
            },
            _ => {
                return Err(SessionError::InvalidHookInput(format!(
                    "unknown argument: {argument}"
                )));
            },
        }
    }

    if from_hook_stdin {
        return Ok(Args {
            transcript_path: transcript_path.unwrap_or_default(),
            store_root,
            workspace_slug: workspace_slug
                .unwrap_or_else(|| "default".to_string()),
            trigger: trigger.unwrap_or_else(|| "stop".to_string()),
            from_hook_stdin,
        });
    }

    Ok(Args {
        transcript_path: transcript_path.ok_or_else(|| {
            SessionError::InvalidHookInput(
                "missing --transcript-path".to_string(),
            )
        })?,
        store_root,
        workspace_slug: workspace_slug.ok_or_else(|| {
            SessionError::InvalidHookInput(
                "missing --workspace-slug".to_string(),
            )
        })?,
        trigger: trigger.unwrap_or_else(|| "stop".to_string()),
        from_hook_stdin,
    })
}

fn args_from_hook_stdin(mut args: Args) -> Result<Args, SessionError> {
    let mut stdin = String::new();
    std::io::stdin()
        .read_to_string(&mut stdin)
        .map_err(|error| {
            SessionError::InvalidHookInput(format!(
                "failed reading hook stdin: {error}"
            ))
        })?;

    if stdin.trim().is_empty() {
        return Ok(args);
    }

    let payload: Value = serde_json::from_str(&stdin).map_err(|error| {
        SessionError::InvalidHookInput(format!(
            "invalid hook stdin json: {error}"
        ))
    })?;

    if let Some(transcript_path) =
        get_hook_field(&payload, &["transcript_path", "transcriptPath"])
    {
        args.transcript_path = PathBuf::from(transcript_path);
    }
    if let Some(workspace_slug) =
        get_hook_field(&payload, &["workspace_slug", "workspaceSlug"])
    {
        args.workspace_slug = workspace_slug;
    }

    if let Some(trigger) =
        get_hook_field(&payload, &["hook_event_name", "hookEventName"])
    {
        args.trigger = normalize_trigger(&trigger);
    }

    Ok(args)
}

fn get_hook_field(
    payload: &Value,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        let Some(value) = payload.get(*key) else {
            continue;
        };
        if let Some(text) = value.as_str() {
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed != "null" {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn normalize_trigger(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        "stop".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_transcript_path(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy().trim().to_string();
    if raw.is_empty() {
        return PathBuf::new();
    }

    #[cfg(windows)]
    {
        if let Some(converted) = wsl_mount_to_windows_path(&raw) {
            return PathBuf::from(converted);
        }
    }

    PathBuf::from(raw)
}

#[cfg(windows)]
fn wsl_mount_to_windows_path(raw: &str) -> Option<String> {
    let trimmed = raw.replace('\\', "/");

    if let Some(rest) = trimmed.strip_prefix("/mnt/") {
        let mut chars = rest.chars();
        let drive = chars.next()?;
        if !drive.is_ascii_alphabetic() {
            return None;
        }
        let remainder = chars.as_str().strip_prefix('/')?;
        return Some(format!(
            "{}:\\{}",
            drive.to_ascii_uppercase(),
            remainder.replace('/', "\\")
        ));
    }

    if let Some(rest) = trimmed.strip_prefix('/') {
        let mut chars = rest.chars();
        let drive = chars.next()?;
        if !drive.is_ascii_alphabetic() {
            return None;
        }
        let remainder = chars.as_str().strip_prefix('/')?;
        return Some(format!(
            "{}:\\{}",
            drive.to_ascii_uppercase(),
            remainder.replace('/', "\\")
        ));
    }

    None
}

fn next_value(
    arguments: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, SessionError> {
    arguments.next().ok_or_else(|| {
        SessionError::InvalidHookInput(format!("missing value for {flag}"))
    })
}

fn print_usage() {
    println!(
        "Usage: copilot-capture-hook (session sync ingest) [--from-hook-stdin] [--transcript-path <PATH>] [--store-root <PATH>] [--workspace-slug <SLUG>] [--trigger <NAME>]"
    );
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use std::path::PathBuf;

    use super::{
        normalize_transcript_path,
        resolve_store_root,
    };

    #[test]
    fn resolve_store_root_uses_explicit_path_when_present() {
        let explicit = PathBuf::from("C:/repo/.session");

        assert_eq!(resolve_store_root(Some(explicit.clone()), None), explicit);
    }

    #[test]
    fn resolve_store_root_defaults_to_hidden_store_in_current_directory() {
        let cwd = tempdir().unwrap();

        let resolved = resolve_store_root(None, Some(cwd.path()));

        assert_eq!(resolved, cwd.path().join(".session"));
    }

    #[test]
    fn resolve_store_root_walks_up_to_ancestor_store() {
        let repo = tempdir().unwrap();
        let nested = repo.path().join("memory-viewers").join("memory-api");
        std::fs::create_dir_all(repo.path().join(".session")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let resolved = resolve_store_root(None, Some(&nested));

        assert_eq!(resolved, repo.path().join(".session"));
    }

    #[test]
    fn resolve_store_root_does_not_descend_into_submodules() {
        let repo = tempdir().unwrap();
        let memory_api = repo.path().join("memory-viewers").join("memory-api");
        std::fs::create_dir_all(memory_api.join(".session")).unwrap();

        // Running from repo root: must NOT descend into the submodule — creates at CWD.
        let resolved = resolve_store_root(None, Some(repo.path()));

        assert_eq!(resolved, repo.path().join(".session"));
    }

    #[test]
    fn normalize_transcript_path_keeps_plain_paths() {
        let path = PathBuf::from("C:/repo/transcript.jsonl");
        let normalized = normalize_transcript_path(&path);
        assert!(!normalized.as_os_str().is_empty());
    }
}
