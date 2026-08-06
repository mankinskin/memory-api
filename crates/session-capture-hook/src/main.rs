use std::{
    path::{
        Path,
        PathBuf,
    },
    process,
};

use session_api::{
    FeedbackSignalKind,
    FollowUpSynthesisOutcome,
    SessionError,
    SessionStoreConfig,
    SessionStorePlan,
    ToolMetricsWindow,
    ToolResponseOverride,
    build_follow_up_ticket_draft,
    mine_explicit_ingestion_signals,
    mine_failed_tool_call_signals,
    mine_structured_feedback_signals,
    synthesize_follow_up_ticket,
};
use session_workspace_resolver::{
    RepositoryRoot,
    ResolveRequest,
    ResolverConfig,
    SessionWorkspaceResolver,
    SessionWorktreeRegistry,
};
use ticket_api::storage::TicketStore;

mod args;

use args::{
    args_from_hook_stdin,
    normalize_transcript_path,
    parse_args,
    print_usage,
};

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

    initialize_session_routing(&args.trigger, args.session_id.as_deref());

    let transcript_path = normalize_transcript_path(&args.transcript_path);
    if !transcript_path.is_file() {
        eprintln!(
            "[copilot-capture-hook] skip: transcript not found at {}",
            transcript_path.display()
        );
        println!("{{}}");
        return Ok(());
    }

    let Some(store_root) = resolve_capture_store_root(
        args.store_root,
        &args.workspace_slug,
        args.session_id.as_deref(),
    ) else {
        println!("{{}}");
        return Ok(());
    };
    let config =
        SessionStoreConfig::new(store_root.clone(), args.workspace_slug);

    let tool_response_override = build_tool_response_override(
        args.tool_call_id.as_deref(),
        args.tool_response_chars,
        args.session_id.as_deref(),
        &transcript_path,
    );
    let plan = config.capture_copilot_transcript_with_tool_response(
        transcript_path,
        args.trigger,
        tool_response_override,
    )?;
    report_structured_feedback_signals(&plan);
    synthesize_follow_up_tickets(
        &plan,
        memory_api::workspace::working_dir().as_deref(),
    );

    // Best-effort worktree/branch/ticket-id inference from the current git
    // environment (ticket bba9b313): must never fail capture — a lost
    // session record would be a far worse bug than the linkage this fixes.
    match memory_api::workspace::working_dir() {
        Some(working_dir) => {
            if let Err(error) = config.infer_worktree_from_environment(
                &plan.record.session_id,
                &working_dir,
            ) {
                eprintln!(
                    "[copilot-capture-hook] worktree/ticket inference skipped: {error}"
                );
            }
        },
        None => eprintln!(
            "[copilot-capture-hook] worktree/ticket inference skipped: no working directory available"
        ),
    }

    // Refresh tool metrics rollup (best-effort)
    refresh_tool_metrics_rollup(&config);

    println!("{{}}");
    Ok(())
}

fn initialize_session_routing(
    trigger: &str,
    session_id: Option<&str>,
) {
    if !trigger.eq_ignore_ascii_case("UserPromptSubmit") {
        return;
    }
    let Some(session_id) =
        session_id.filter(|session_id| !session_id.trim().is_empty())
    else {
        eprintln!(
            "[copilot-capture-hook] session routing skipped: hook payload has no session id"
        );
        return;
    };
    let current_dir = match std::env::current_dir() {
        Ok(current_dir) => current_dir,
        Err(error) => {
            eprintln!(
                "[copilot-capture-hook] session routing skipped: could not determine current directory: {error}"
            );
            return;
        },
    };
    let worktree = match RepositoryRoot::new(&current_dir) {
        Ok(worktree) => worktree,
        Err(error) => {
            eprintln!(
                "[copilot-capture-hook] session routing skipped: could not resolve current checkout: {error}"
            );
            return;
        },
    };
    let main_checkout = match std::env::var_os("MCP_MAIN_CHECKOUT") {
        Some(path) => RepositoryRoot::new(PathBuf::from(path)),
        None => Ok(worktree.clone()),
    };
    let main_checkout = match main_checkout {
        Ok(main_checkout) => main_checkout,
        Err(error) => {
            eprintln!(
                "[copilot-capture-hook] session routing skipped: could not resolve main checkout: {error}"
            );
            return;
        },
    };
    if let Err(error) = SessionWorktreeRegistry::new(main_checkout)
        .upsert(session_id, worktree.as_path())
    {
        eprintln!(
            "[copilot-capture-hook] session routing skipped: could not register session {session_id}: {error}"
        );
    }
}

/// Build the layered output-size override for the tool call that triggered
/// this hook invocation (ticket 44119807 T2).
///
/// The hook stdin's `tool_use_id` is the full on-disk spill entry name,
/// `<bare_id>__vscode-<epoch>`, while the transcript's own `toolCallId` is
/// the bare id without that suffix. The two must be split apart: the bare id
/// is what `apply_tool_response_override` matches against transcript events,
/// while the full suffixed id is the literal spill directory name.
///
/// Layer 1 (`hook_payload`): the hook stdin's `tool_response` string, used
/// only when non-empty (observed to be populated for some tool types, e.g.
/// `run_in_terminal`, and empty for others, e.g. `read_file`).
///
/// Layer 2 (`spill_file`): VS Code Copilot Chat spills large tool outputs to
/// `<workspaceStorage>/<hash>/GitHub.copilot-chat/chat-session-resources/
/// <session_id>/<tool_use_id>/content.txt` (or `content.json`), derived here
/// from the hook stdin's own `transcript_path` (its
/// `GitHub.copilot-chat/transcripts/<session>.jsonl` layout shares the same
/// `GitHub.copilot-chat` root) plus `session_id` and the full `tool_use_id`.
fn build_tool_response_override(
    tool_use_id: Option<&str>,
    tool_response_chars: Option<u64>,
    session_id: Option<&str>,
    transcript_path: &Path,
) -> Option<ToolResponseOverride> {
    let tool_use_id = tool_use_id?;
    let bare_tool_call_id =
        tool_use_id.split("__vscode-").next().unwrap_or(tool_use_id);

    if let Some(output_chars) = tool_response_chars.filter(|chars| *chars > 0) {
        return Some(ToolResponseOverride {
            tool_call_id: bare_tool_call_id.to_string(),
            output_chars,
            output_source: "hook_payload".to_string(),
        });
    }

    let session_id = session_id?;
    let output_chars =
        stat_spill_output_chars(transcript_path, session_id, tool_use_id)?;
    Some(ToolResponseOverride {
        tool_call_id: bare_tool_call_id.to_string(),
        output_chars,
        output_source: "spill_file".to_string(),
    })
}

/// Stat the `chat-session-resources/<session_id>/<tool_use_id>` spill entry
/// relative to the hook stdin's `transcript_path`
/// (`.../GitHub.copilot-chat/transcripts/<session>.jsonl`). `tool_use_id` is
/// the full suffixed id (`<bare_id>__vscode-<epoch>`), matching the literal
/// on-disk directory name. Returns `None` (unmeasured, never a fabricated
/// zero) when the root can't be derived or no spill file is found.
///
/// VS Code writes the spill file asynchronously after invoking the
/// PostToolUse hook, so the file can be briefly absent at hook-fire time;
/// this retries a few times with a short backoff before giving up.
fn stat_spill_output_chars(
    transcript_path: &Path,
    session_id: &str,
    tool_use_id: &str,
) -> Option<u64> {
    let chat_root = transcript_path.parent()?.parent()?;
    let entry_dir = chat_root
        .join("chat-session-resources")
        .join(session_id)
        .join(tool_use_id);

    const MAX_ATTEMPTS: u32 = 5;
    const RETRY_DELAY: std::time::Duration =
        std::time::Duration::from_millis(100);
    for attempt in 0..MAX_ATTEMPTS {
        if let Some(candidate) = ["content.txt", "content.json"]
            .iter()
            .map(|name| entry_dir.join(name))
            .find(|path| path.is_file())
        {
            let bytes = std::fs::read(&candidate).ok()?;
            return Some(String::from_utf8_lossy(&bytes).chars().count() as u64);
        }
        if attempt + 1 < MAX_ATTEMPTS {
            std::thread::sleep(RETRY_DELAY);
        }
    }
    None
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
            plan.record.session_id,
            signals.len(),
            failed_tool_calls,
            explicit_ingestions,
            json
        ),
        Err(error) => eprintln!(
            "[copilot-capture-hook] structured feedback signals for session {}: {} total ({} failed tool calls, {} explicit ingestions); summary serialization failed: {error}",
            plan.record.session_id,
            signals.len(),
            failed_tool_calls,
            explicit_ingestions
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

/// Refresh the tool metrics rollup for the store after a successful capture.
/// Best-effort: rollup write failures do NOT fail the capture.
fn refresh_tool_metrics_rollup(config: &SessionStoreConfig) {
    let window = ToolMetricsWindow::default();
    if let Err(error) = config.write_tool_metrics_rollup(window) {
        eprintln!(
            "[copilot-capture-hook] tool metrics rollup refresh failed (non-fatal): {error}"
        );
    }
}

fn resolve_capture_store_root(
    store_root: Option<PathBuf>,
    workspace_slug: &str,
    session_id: Option<&str>,
) -> Option<PathBuf> {
    if let Some(store_root) = store_root {
        return Some(store_root);
    }

    let Some(main_checkout) = std::env::var_os("MCP_MAIN_CHECKOUT") else {
        eprintln!(
            "[copilot-capture-hook] capture skipped: MCP_MAIN_CHECKOUT is unset; refusing to write a default .session store"
        );
        return None;
    };
    let Some(session_id) =
        session_id.filter(|session_id| !session_id.trim().is_empty())
    else {
        eprintln!(
            "[copilot-capture-hook] capture skipped: hook payload has no session id; refusing to write a default .session store"
        );
        return None;
    };
    let resolver = match SessionWorkspaceResolver::new(ResolverConfig {
        main_checkout: PathBuf::from(main_checkout),
        workspace_slug: workspace_slug.to_string(),
    }) {
        Ok(resolver) => resolver,
        Err(error) => {
            eprintln!(
                "[copilot-capture-hook] capture skipped: could not configure session workspace resolver: {error}"
            );
            return None;
        },
    };
    match resolver.resolve(ResolveRequest {
        session_id,
        relative_workspace: None,
        store_dir: ".session",
    }) {
        Ok(workspace) => match workspace.store_root(".session") {
            Ok(store_root) => Some(store_root),
            Err(error) => {
                eprintln!(
                    "[copilot-capture-hook] capture skipped: could not resolve worktree session store: {error}"
                );
                None
            },
        },
        Err(error) => {
            eprintln!(
                "[copilot-capture-hook] capture skipped: no active worktree assignment for session {session_id}: {error}"
            );
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        path::{
            Path,
            PathBuf,
        },
        sync::Mutex,
    };

    use serde_json::Value;
    use session_api::{
        SessionStoreConfig,
        SessionWorktreeCheckInRequest,
    };
    use session_workspace_resolver::{
        RepositoryRoot,
        SessionWorktreeRegistry,
    };
    use tempfile::tempdir;

    use super::{
        initialize_session_routing,
        resolve_capture_store_root,
    };
    use crate::args::normalize_transcript_path;

    static CWD_LOCK: Mutex<()> = Mutex::new(());
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn register_active_worktree(
        main_checkout: &Path,
        session_id: &str,
    ) -> PathBuf {
        let worktree = main_checkout.join(".worktrees").join("capture");
        std::fs::create_dir_all(&worktree).unwrap();
        SessionStoreConfig::new(worktree.join(".session"), "default")
            .check_in_worktree(SessionWorktreeCheckInRequest {
                session_id: session_id.to_string(),
                owner_id: "agent".to_string(),
                ticket_id: "ticket".to_string(),
                worktree_path: worktree.clone(),
                branch: "agent/40349f3f-capture-routing".to_string(),
                predecessor_session_id: None,
            })
            .unwrap();
        SessionWorktreeRegistry::new(
            RepositoryRoot::new(main_checkout).unwrap(),
        )
        .upsert(session_id, &worktree)
        .unwrap();
        worktree
    }

    #[test]
    fn capture_writes_to_worktree_store_while_cwd_is_main_checkout() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree =
            register_active_worktree(&main_checkout, "session-worktree");
        std::fs::create_dir_all(main_checkout.join(".session")).unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        let original_main_checkout = env::var_os("MCP_MAIN_CHECKOUT");
        unsafe { env::set_var("MCP_MAIN_CHECKOUT", &main_checkout) };
        std::env::set_current_dir(&main_checkout).unwrap();

        let result = resolve_capture_store_root(
            None,
            "default",
            Some("session-worktree"),
        );

        std::env::set_current_dir(original_cwd).unwrap();
        unsafe {
            match original_main_checkout {
                Some(value) => env::set_var("MCP_MAIN_CHECKOUT", value),
                None => env::remove_var("MCP_MAIN_CHECKOUT"),
            }
        }
        let store_root =
            result.expect("active worktree assignment should resolve");

        assert_eq!(store_root, worktree.join(".session"));
        assert!(
            std::fs::read_dir(main_checkout.join(".session"))
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn capture_without_assignment_warns_and_does_not_write_main_checkout() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        std::fs::create_dir_all(main_checkout.join(".session")).unwrap();
        let original_main_checkout = env::var_os("MCP_MAIN_CHECKOUT");
        unsafe { env::set_var("MCP_MAIN_CHECKOUT", &main_checkout) };

        assert_eq!(
            resolve_capture_store_root(None, "default", Some("missing")),
            None
        );
        unsafe {
            match original_main_checkout {
                Some(value) => env::set_var("MCP_MAIN_CHECKOUT", value),
                None => env::remove_var("MCP_MAIN_CHECKOUT"),
            }
        }
        assert!(
            std::fs::read_dir(main_checkout.join(".session"))
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn capture_with_inactive_assignment_does_not_write_main_checkout() {
        assert_eq!(
            resolve_capture_store_root(None, "default", Some("inactive")),
            None
        );
    }

    #[test]
    fn capture_store_resolution_ignores_process_current_directory() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree =
            register_active_worktree(&main_checkout, "session-third-cwd");
        let unrelated = fixture.path().join("unrelated");
        std::fs::create_dir_all(&unrelated).unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        let original_main_checkout = env::var_os("MCP_MAIN_CHECKOUT");
        unsafe { env::set_var("MCP_MAIN_CHECKOUT", &main_checkout) };
        std::env::set_current_dir(&unrelated).unwrap();

        let result = resolve_capture_store_root(
            None,
            "default",
            Some("session-third-cwd"),
        );

        std::env::set_current_dir(original_cwd).unwrap();
        unsafe {
            match original_main_checkout {
                Some(value) => env::set_var("MCP_MAIN_CHECKOUT", value),
                None => env::remove_var("MCP_MAIN_CHECKOUT"),
            }
        }
        assert_eq!(result, Some(worktree.join(".session")));
    }

    #[test]
    fn normalize_transcript_path_keeps_plain_paths() {
        let path = PathBuf::from("C:/repo/transcript.jsonl");
        let normalized = normalize_transcript_path(&path);
        assert!(!normalized.as_os_str().is_empty());
    }

    fn read_registry_entries(
        main_checkout: &Path
    ) -> serde_json::Map<String, Value> {
        let contents = std::fs::read_to_string(
            main_checkout.join(".session-routing/worktree-index.json"),
        )
        .unwrap();
        serde_json::from_str::<Value>(&contents)
            .unwrap()
            .get("entries")
            .and_then(Value::as_object)
            .cloned()
            .unwrap()
    }

    fn run_user_prompt_submit(
        main_checkout: &Path,
        worktree: &Path,
        session_id: Option<&str>,
    ) {
        let original_cwd = env::current_dir().unwrap();
        let original_main_checkout = env::var_os("MCP_MAIN_CHECKOUT");
        unsafe { env::set_var("MCP_MAIN_CHECKOUT", main_checkout) };
        env::set_current_dir(worktree).unwrap();

        initialize_session_routing("UserPromptSubmit", session_id);

        env::set_current_dir(original_cwd).unwrap();
        unsafe {
            match original_main_checkout {
                Some(value) => env::set_var("MCP_MAIN_CHECKOUT", value),
                None => env::remove_var("MCP_MAIN_CHECKOUT"),
            }
        }
    }

    #[test]
    fn user_prompt_submit_writes_session_worktree_registry_entry() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        run_user_prompt_submit(&main_checkout, &worktree, Some("session-one"));

        let entries = read_registry_entries(&main_checkout);
        let entry = entries.get("session-one").unwrap();
        assert_eq!(
            PathBuf::from(entry["worktree_path"].as_str().unwrap()),
            RepositoryRoot::new(&worktree).unwrap().as_path()
        );
    }

    #[test]
    fn user_prompt_submit_is_idempotent_for_a_session() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        run_user_prompt_submit(&main_checkout, &worktree, Some("session-one"));
        run_user_prompt_submit(&main_checkout, &worktree, Some("session-one"));

        let entries = read_registry_entries(&main_checkout);
        assert_eq!(entries.len(), 1);
        assert!(
            !entries["session-one"]["updated_at"]
                .as_str()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn user_prompt_submit_records_distinct_sessions() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        run_user_prompt_submit(&main_checkout, &worktree, Some("session-one"));
        run_user_prompt_submit(&main_checkout, &worktree, Some("session-two"));

        let entries = read_registry_entries(&main_checkout);
        assert_eq!(entries.len(), 2);
        assert!(entries.contains_key("session-one"));
        assert!(entries.contains_key("session-two"));
    }

    #[test]
    fn user_prompt_submit_without_session_id_does_not_write_registry() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        run_user_prompt_submit(&main_checkout, &worktree, None);

        assert!(
            !main_checkout
                .join(".session-routing/worktree-index.json")
                .exists()
        );
    }

    #[test]
    fn stop_does_not_write_session_worktree_registry() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        let original_cwd = env::current_dir().unwrap();
        let original_main_checkout = env::var_os("MCP_MAIN_CHECKOUT");
        unsafe { env::set_var("MCP_MAIN_CHECKOUT", &main_checkout) };
        env::set_current_dir(&worktree).unwrap();

        initialize_session_routing("Stop", Some("session-one"));

        env::set_current_dir(original_cwd).unwrap();
        unsafe {
            match original_main_checkout {
                Some(value) => env::set_var("MCP_MAIN_CHECKOUT", value),
                None => env::remove_var("MCP_MAIN_CHECKOUT"),
            }
        }
        assert!(
            !main_checkout
                .join(".session-routing/worktree-index.json")
                .exists()
        );
    }

    #[test]
    fn user_prompt_submit_swallows_main_checkout_resolution_failure() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let worktree = fixture.path().join("worktree");
        let invalid_main_checkout = fixture.path().join("missing-main");
        std::fs::create_dir_all(&worktree).unwrap();
        let original_cwd = env::current_dir().unwrap();
        let original_main_checkout = env::var_os("MCP_MAIN_CHECKOUT");
        unsafe { env::set_var("MCP_MAIN_CHECKOUT", &invalid_main_checkout) };
        env::set_current_dir(&worktree).unwrap();

        initialize_session_routing("UserPromptSubmit", Some("session-one"));

        env::set_current_dir(original_cwd).unwrap();
        unsafe {
            match original_main_checkout {
                Some(value) => env::set_var("MCP_MAIN_CHECKOUT", value),
                None => env::remove_var("MCP_MAIN_CHECKOUT"),
            }
        }
        assert!(
            !invalid_main_checkout
                .join(".session-routing/worktree-index.json")
                .exists()
        );
    }
}
