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
    ResolveRequest,
    ResolverConfig,
    SessionWorkspaceResolver,
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

    // Best-effort worktree/branch/ticket-id inference from the resolved
    // session store's parent (ticket bba9b313): must never fail capture — a lost
    // session record would be a far worse bug than the linkage this fixes.
    if store_root.parent().is_none() {
        eprintln!(
            "[copilot-capture-hook] worktree/ticket inference skipped: resolved session store has no parent"
        );
    } else if let Err(error) = infer_capture_worktree(
        &config,
        &plan.record.session_id,
        &store_root,
    ) {
        eprintln!(
            "[copilot-capture-hook] worktree/ticket inference skipped: {error}"
        );
    }

    // Refresh tool metrics rollup (best-effort)
    refresh_tool_metrics_rollup(&config);

    println!("{{}}");
    Ok(())
}

/// Resolves the checkout the hook was launched in, which anchors the session
/// store that records worktree assignments.
///
/// `MCP_MAIN_CHECKOUT` stays available as an override for callers that cannot
/// control the working directory, but it is not required.
fn anchor_checkout(current_dir: &Path) -> PathBuf {
    std::env::var_os("MCP_MAIN_CHECKOUT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| current_dir.to_path_buf())
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
    let anchor = anchor_checkout(&current_dir);
    if !anchor.is_dir() {
        eprintln!(
            "[copilot-capture-hook] session routing skipped: anchor checkout '{}' does not exist",
            anchor.display()
        );
        return;
    }
    let resolver = match SessionWorkspaceResolver::new(ResolverConfig {
        main_checkout: anchor.clone(),
        workspace_slug: "default".to_string(),
    }) {
        Ok(resolver) => resolver,
        Err(error) => {
            eprintln!(
                "[copilot-capture-hook] session routing skipped: could not configure session workspace resolver: {error}"
            );
            return;
        },
    };
    let workspace = match resolver.resolve(ResolveRequest {
        session_id,
        relative_workspace: None,
        store_dir: ".session",
    }) {
        Ok(workspace) if workspace.is_worktree() => workspace,
        Ok(_) => {
            eprintln!(
                "[copilot-capture-hook] session routing skipped: resolver selected the main checkout for session {session_id}"
            );
            return;
        },
        Err(error) => {
            eprintln!(
                "[copilot-capture-hook] session routing skipped: no active worktree assignment for session {session_id}: {error}"
            );
            return;
        },
    };
    let config = SessionStoreConfig::new(anchor.join(".session"), "default");
    let worktree = workspace.target_root();
    if let Err(error) = config.replace_main_worktree_inference(
        session_id,
        &anchor,
        worktree,
    ) {
        eprintln!(
            "[copilot-capture-hook] session routing skipped: could not repair a main-checkout assignment for session {session_id}: {error}"
        );
        return;
    }
    if let Err(error) =
        config.infer_worktree_from_environment(session_id, worktree)
    {
        eprintln!(
            "[copilot-capture-hook] session routing skipped: could not assign a worktree for session {session_id}: {error}"
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

    let Some(session_id) =
        session_id.filter(|session_id| !session_id.trim().is_empty())
    else {
        eprintln!(
            "[copilot-capture-hook] capture skipped: hook payload has no session id; refusing to write a default .session store"
        );
        return None;
    };
    let current_dir = match std::env::current_dir() {
        Ok(current_dir) => current_dir,
        Err(error) => {
            eprintln!(
                "[copilot-capture-hook] capture skipped: could not determine current directory: {error}"
            );
            return None;
        },
    };
    let anchor = anchor_checkout(&current_dir);
    if !anchor.join(".session").is_dir() {
        eprintln!(
            "[copilot-capture-hook] capture skipped: no session store beneath '{}'; refusing to write a default .session store",
            anchor.display()
        );
        return None;
    }
    let resolver = match SessionWorkspaceResolver::new(ResolverConfig {
        main_checkout: anchor,
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

fn infer_capture_worktree(
    config: &SessionStoreConfig,
    session_id: &str,
    store_root: &Path,
) -> Result<(), SessionError> {
    let Some(worktree_root) = store_root.parent() else {
        return Ok(());
    };
    config.infer_worktree_from_environment(session_id, worktree_root)
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

    use session_api::{
        SessionStoreConfig,
        SessionWorktreeCheckInRequest,
    };
    use tempfile::tempdir;

    use super::{
        infer_capture_worktree,
        initialize_session_routing,
        resolve_capture_store_root,
    };
    use crate::args::normalize_transcript_path;

    static CWD_LOCK: Mutex<()> = Mutex::new(());
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    enum CheckoutFixtureKind {
        Main,
        LinkedWorktree,
    }

    fn create_checkout(
        path: &Path,
        kind: CheckoutFixtureKind,
    ) {
        std::fs::create_dir_all(path).unwrap();
        match kind {
            CheckoutFixtureKind::Main => {
                std::fs::create_dir_all(path.join(".git")).unwrap();
            },
            CheckoutFixtureKind::LinkedWorktree => {
                std::fs::write(
                    path.join(".git"),
                    format!(
                        "gitdir: {}\n",
                        path.join(".git-worktree").display()
                    ),
                )
                .unwrap();
            },
        }
    }

    fn register_active_worktree(
        main_checkout: &Path,
        session_id: &str,
    ) -> PathBuf {
        create_checkout(main_checkout, CheckoutFixtureKind::Main);
        let worktree = main_checkout.join(".worktrees").join("capture");
        create_checkout(&worktree, CheckoutFixtureKind::LinkedWorktree);
        SessionStoreConfig::new(main_checkout.join(".session"), "default")
            .check_in_worktree(SessionWorktreeCheckInRequest {
                session_id: session_id.to_string(),
                owner_id: "agent".to_string(),
                ticket_id: "ticket".to_string(),
                worktree_path: worktree.clone(),
                branch: "agent/40349f3f-capture-routing".to_string(),
                predecessor_session_id: None,
            })
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
        // The anchor store legitimately holds the worktree assignment, so the
        // guarantee is that capture is routed away from it, not that it is
        // empty.
        assert_ne!(store_root, main_checkout.join(".session"));
    }

    #[test]
    fn capture_without_assignment_warns_and_does_not_write_main_checkout() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        create_checkout(&main_checkout, CheckoutFixtureKind::Main);
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
    fn capture_inference_uses_resolved_store_parent_not_process_directory() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join("worktree");
        create_git_worktree(&main_checkout, &worktree, "feature");
        let store_root = worktree.join(".session");
        let config = SessionStoreConfig::new(&store_root, "default");
        let original_cwd = env::current_dir().unwrap();
        env::set_current_dir(&main_checkout).unwrap();

        infer_capture_worktree(&config, "session-store-parent", &store_root)
            .unwrap();

        env::set_current_dir(original_cwd).unwrap();
        let record = config.read_session("session-store-parent").unwrap();
        assert_eq!(
            record.metadata.worktree.unwrap().path.canonicalize().unwrap(),
            worktree.canonicalize().unwrap()
        );
    }

    #[test]
    fn normalize_transcript_path_keeps_plain_paths() {
        let path = PathBuf::from("C:/repo/transcript.jsonl");
        let normalized = normalize_transcript_path(&path);
        assert!(!normalized.as_os_str().is_empty());
    }

    fn git(
        args: &[&str],
        cwd: &Path,
    ) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed in {}", cwd.display());
    }

    /// Creates a real repository plus a real linked worktree.
    ///
    /// Worktree inference shells out to `git rev-parse`, so a fixture that only
    /// fabricates a `.git` entry would silently no-op instead of assigning.
    fn create_git_worktree(
        main_checkout: &Path,
        worktree: &Path,
        branch: &str,
    ) {
        std::fs::create_dir_all(main_checkout).unwrap();
        git(&["init", "--quiet"], main_checkout);
        git(&["config", "user.email", "hook@example.com"], main_checkout);
        git(&["config", "user.name", "hook"], main_checkout);
        git(
            &["commit", "--quiet", "--allow-empty", "-m", "init"],
            main_checkout,
        );
        git(
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                branch,
                worktree.to_str().unwrap(),
            ],
            main_checkout,
        );
    }

    fn assigned_worktree(
        main_checkout: &Path,
        session_id: &str,
    ) -> Option<PathBuf> {
        SessionStoreConfig::new(main_checkout.join(".session"), "default")
            .lookup_worktree(session_id)
            .ok()
            .map(|receipt| receipt.worktree_path)
    }

    fn run_user_prompt_submit(
        main_checkout: &Path,
        process_directory: &Path,
        session_id: Option<&str>,
    ) {
        let original_cwd = env::current_dir().unwrap();
        let original_main_checkout = env::var_os("MCP_MAIN_CHECKOUT");
        unsafe { env::set_var("MCP_MAIN_CHECKOUT", main_checkout) };
        env::set_current_dir(process_directory).unwrap();

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
    fn user_prompt_submit_discovers_the_session_worktree_from_main_cwd() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join(".worktrees").join("abcdefgh-routing");
        create_git_worktree(&main_checkout, &worktree, "feature");

        run_user_prompt_submit(&main_checkout, &main_checkout, Some("abcdefgh-session"));

        let assigned = assigned_worktree(&main_checkout, "abcdefgh-session")
            .expect("user prompt submit should assign a worktree");
        assert_eq!(
            assigned.canonicalize().unwrap(),
            worktree.canonicalize().unwrap()
        );
        assert_eq!(
            SessionStoreConfig::new(main_checkout.join(".session"), "default")
                .lookup_worktree("abcdefgh-session")
                .unwrap()
                .branch,
            "feature"
        );
    }

    #[test]
    fn user_prompt_submit_is_idempotent_for_a_session() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join(".worktrees").join("abcdefgh-routing");
        create_git_worktree(&main_checkout, &worktree, "feature");

        run_user_prompt_submit(&main_checkout, &main_checkout, Some("abcdefgh-session"));
        let first = assigned_worktree(&main_checkout, "abcdefgh-session").unwrap();
        run_user_prompt_submit(&main_checkout, &main_checkout, Some("abcdefgh-session"));
        let second = assigned_worktree(&main_checkout, "abcdefgh-session").unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn user_prompt_submit_records_distinct_sessions() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let first_worktree = main_checkout.join(".worktrees").join("abcdefgh-routing");
        let second_worktree = main_checkout.join(".worktrees").join("ijklmnop-routing");
        create_git_worktree(&main_checkout, &first_worktree, "feature-one");
        create_git_worktree(&main_checkout, &second_worktree, "feature-two");

        run_user_prompt_submit(&main_checkout, &main_checkout, Some("abcdefgh-session"));
        run_user_prompt_submit(&main_checkout, &main_checkout, Some("ijklmnop-session"));

        assert!(assigned_worktree(&main_checkout, "abcdefgh-session").is_some());
        assert!(assigned_worktree(&main_checkout, "ijklmnop-session").is_some());
    }

    #[test]
    fn user_prompt_submit_without_discoverable_worktree_does_not_assign_main() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        create_git_worktree(
            &main_checkout,
            &main_checkout.join("unrelated-worktree"),
            "feature",
        );

        run_user_prompt_submit(&main_checkout, &main_checkout, Some("abcdefgh-session"));

        assert!(assigned_worktree(&main_checkout, "abcdefgh-session").is_none());
    }

    #[test]
    fn user_prompt_submit_replaces_a_stale_main_checkout_assignment() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join(".worktrees").join("abcdefgh-routing");
        create_git_worktree(&main_checkout, &worktree, "feature");
        let config = SessionStoreConfig::new(main_checkout.join(".session"), "default");
        config
            .check_in_worktree(SessionWorktreeCheckInRequest {
                session_id: "abcdefgh-session".to_string(),
                owner_id: "agent".to_string(),
                ticket_id: "ticket".to_string(),
                worktree_path: main_checkout.clone(),
                branch: "main".to_string(),
                predecessor_session_id: None,
            })
            .unwrap();

        run_user_prompt_submit(&main_checkout, &main_checkout, Some("abcdefgh-session"));

        let assignment = config.lookup_worktree("abcdefgh-session").unwrap();
        assert_eq!(
            assignment.worktree_path.canonicalize().unwrap(),
            worktree.canonicalize().unwrap()
        );
        assert_eq!(assignment.branch, "feature");
    }

    #[test]
    fn user_prompt_submit_without_session_id_does_not_assign() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join("worktree");
        create_git_worktree(&main_checkout, &worktree, "feature");

        run_user_prompt_submit(&main_checkout, &worktree, None);

        assert!(!main_checkout.join(".session").exists());
    }

    #[test]
    fn stop_does_not_assign_a_session_worktree() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join("worktree");
        create_git_worktree(&main_checkout, &worktree, "feature");
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
        assert!(!main_checkout.join(".session").exists());
    }

    #[test]
    fn user_prompt_submit_skips_a_missing_anchor_override() {
        let _cwd_lock = CWD_LOCK.lock().unwrap();
        let _env_lock = ENV_LOCK.lock().unwrap();
        let fixture = tempdir().unwrap();
        let main_checkout = fixture.path().join("main");
        let worktree = main_checkout.join("worktree");
        let invalid_main_checkout = fixture.path().join("missing-main");
        create_git_worktree(&main_checkout, &worktree, "feature");
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
        assert!(!invalid_main_checkout.exists());
    }
}
