use std::{
    env,
    io::Read,
    path::{
        Path,
        PathBuf,
    },
};

use serde_json::Value;
use session_api::SessionError;

pub(super) struct Args {
    pub(super) transcript_path: PathBuf,
    pub(super) store_root: Option<PathBuf>,
    pub(super) workspace_slug: String,
    pub(super) trigger: String,
    pub(super) from_hook_stdin: bool,
    /// PostToolUse hook stdin `tool_use_id`, paired with `tool_response_chars`
    /// to build a `ToolResponseOverride` (ticket 44119807 T2).
    pub(super) tool_call_id: Option<String>,
    /// Char count of the PostToolUse hook stdin `tool_response`. `Some(0)` is
    /// a real zero-length measurement, distinct from `None` (field absent).
    pub(super) tool_response_chars: Option<u64>,
    /// PostToolUse hook stdin `session_id`, used with `transcript_path` and
    /// `tool_call_id` to derive the `chat-session-resources` spill-file path
    /// (ticket 44119807 T2 AC1 real-capture fix): the real hook payload's
    /// `tool_response` is observed to always be empty, so the on-disk spill
    /// convention is the only working source for real sessions.
    pub(super) session_id: Option<String>,
}

pub(super) fn parse_args() -> Result<Args, SessionError> {
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
            "--transcript-path" =>
                transcript_path = Some(PathBuf::from(next_value(
                    &mut arguments,
                    "--transcript-path",
                )?)),
            "--store-root" =>
                store_root = Some(PathBuf::from(next_value(
                    &mut arguments,
                    "--store-root",
                )?)),
            "--workspace-slug" =>
                workspace_slug =
                    Some(next_value(&mut arguments, "--workspace-slug")?),
            "--trigger" =>
                trigger = Some(next_value(&mut arguments, "--trigger")?),
            "--from-hook-stdin" => from_hook_stdin = true,
            _ =>
                return Err(SessionError::InvalidHookInput(format!(
                    "unknown argument: {argument}"
                ))),
        }
    }

    let transcript_path = if from_hook_stdin {
        transcript_path.unwrap_or_default()
    } else {
        transcript_path.ok_or_else(|| {
            SessionError::InvalidHookInput(
                "missing --transcript-path".to_string(),
            )
        })?
    };
    Ok(Args {
        transcript_path,
        store_root,
        workspace_slug: workspace_slug.unwrap_or_else(|| "default".to_string()),
        trigger: trigger.unwrap_or_else(|| "stop".to_string()),
        from_hook_stdin,
        tool_call_id: None,
        tool_response_chars: None,
        session_id: None,
    })
}

pub(super) fn args_from_hook_stdin(
    mut args: Args
) -> Result<Args, SessionError> {
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
    if let Some(path) =
        get_field(&payload, &["transcript_path", "transcriptPath"])
    {
        args.transcript_path = PathBuf::from(path);
    }
    if let Some(workspace_slug) =
        get_field(&payload, &["workspace_slug", "workspaceSlug"])
    {
        args.workspace_slug = workspace_slug;
    }
    if let Some(trigger) =
        get_field(&payload, &["hook_event_name", "hookEventName"])
    {
        args.trigger = normalize_trigger(&trigger);
    }
    if let Some(tool_call_id) =
        get_field(&payload, &["tool_use_id", "toolUseId"])
    {
        args.tool_call_id = Some(tool_call_id);
    }
    if let Some(session_id) =
        get_field(&payload, &["session_id", "sessionId"])
    {
        args.session_id = Some(session_id);
    }
    // `tool_response` presence (even an empty string) is itself a real
    // hook-payload measurement, so this reads the raw field directly instead
    // of `get_field`, which filters out empty strings as absent.
    if let Some(tool_response) = ["tool_response", "toolResponse"]
        .iter()
        .find_map(|key| payload.get(*key)?.as_str())
    {
        args.tool_response_chars = Some(tool_response.chars().count() as u64);
    }
    Ok(args)
}

pub(super) fn normalize_transcript_path(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy().trim().to_string();
    if raw.is_empty() {
        return PathBuf::new();
    }
    #[cfg(windows)]
    if let Some(converted) = wsl_mount_to_windows_path(&raw) {
        return PathBuf::from(converted);
    }
    PathBuf::from(raw)
}

fn get_field(
    payload: &Value,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        payload
            .get(*key)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "null")
            .map(str::to_string)
    })
}

fn normalize_trigger(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        "stop".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(windows)]
fn wsl_mount_to_windows_path(raw: &str) -> Option<String> {
    let trimmed = raw.replace('\\', "/");
    let rest = trimmed
        .strip_prefix("/mnt/")
        .or_else(|| trimmed.strip_prefix('/'))?;
    let mut characters = rest.chars();
    let drive = characters.next()?;
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    let remainder = characters.as_str().strip_prefix('/')?;
    Some(format!(
        "{}:\\{}",
        drive.to_ascii_uppercase(),
        remainder.replace('/', "\\")
    ))
}

fn next_value(
    arguments: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, SessionError> {
    arguments.next().ok_or_else(|| {
        SessionError::InvalidHookInput(format!("missing value for {flag}"))
    })
}

pub(super) fn print_usage() {
    println!(
        "Usage: copilot-capture-hook (session sync ingest) [--from-hook-stdin] [--transcript-path <PATH>] [--store-root <PATH>] [--workspace-slug <SLUG>] [--trigger <NAME>]"
    );
}
