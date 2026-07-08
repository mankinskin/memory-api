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
    SessionError,
    SessionStoreConfig,
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
    let config = SessionStoreConfig::new(store_root, args.workspace_slug);

    config.capture_copilot_transcript(transcript_path, args.trigger)?;
    println!("{{}}");
    Ok(())
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

fn args_from_hook_stdin(
    mut args: Args,
) -> Result<Args, SessionError> {
    let mut stdin = String::new();
    std::io::stdin().read_to_string(&mut stdin).map_err(|error| {
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

    if let Some(transcript_path) = get_hook_field(
        &payload,
        &["transcript_path", "transcriptPath"],
    ) {
        args.transcript_path = PathBuf::from(transcript_path);
    }
    if let Some(workspace_slug) =
        get_hook_field(&payload, &["workspace_slug", "workspaceSlug"])
    {
        args.workspace_slug = workspace_slug;
    }

    if let Some(trigger) = get_hook_field(
        &payload,
        &["hook_event_name", "hookEventName"],
    ) {
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
