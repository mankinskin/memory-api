use std::{
    env,
    path::Path,
    path::PathBuf,
    process,
};

use session_api::{
    SessionError,
    SessionStoreConfig,
};

fn main() {
    match run() {
        Ok(()) => {}
        Err(SessionError::InvalidHookInput(message)) if message == "help" => {
            print_usage();
        }
        Err(error) => {
            eprintln!("[copilot-stop-hook] {error}");
            process::exit(1);
        }
    }
}

fn run() -> Result<(), SessionError> {
    let args = parse_args()?;
    let store_root = resolve_store_root(args.store_root, memory_api::workspace::working_dir().as_deref());
    let config = SessionStoreConfig::new(store_root, args.workspace_slug);

    config.capture_copilot_transcript(args.transcript_path, args.trigger)?;
    Ok(())
}

fn resolve_store_root(
    store_root: Option<PathBuf>,
    cwd: Option<&Path>,
) -> PathBuf {
    match store_root {
        Some(store_root) => store_root,
        None => match cwd {
            Some(cwd) => memory_api::workspace::resolve_local_root_from(cwd, ".session"),
            None => std::path::PathBuf::from(".session"),
        },
    }
}

struct Args {
    transcript_path: PathBuf,
    store_root: Option<PathBuf>,
    workspace_slug: String,
    trigger: String,
}

fn parse_args() -> Result<Args, SessionError> {
    let mut transcript_path = None;
    let mut store_root = None;
    let mut workspace_slug = None;
    let mut trigger = Some("stop".to_string());

    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" => return Err(SessionError::InvalidHookInput("help".to_string())),
            "--transcript-path" => {
                transcript_path = Some(PathBuf::from(next_value(&mut arguments, "--transcript-path")?));
            }
            "--store-root" => {
                store_root = Some(PathBuf::from(next_value(&mut arguments, "--store-root")?));
            }
            "--workspace-slug" => {
                workspace_slug = Some(next_value(&mut arguments, "--workspace-slug")?);
            }
            "--trigger" => {
                trigger = Some(next_value(&mut arguments, "--trigger")?);
            }
            _ => {
                return Err(SessionError::InvalidHookInput(format!(
                    "unknown argument: {argument}"
                )));
            }
        }
    }

    Ok(Args {
        transcript_path: transcript_path.ok_or_else(|| {
            SessionError::InvalidHookInput("missing --transcript-path".to_string())
        })?,
        store_root,
        workspace_slug: workspace_slug.ok_or_else(|| {
            SessionError::InvalidHookInput("missing --workspace-slug".to_string())
        })?,
        trigger: trigger.unwrap_or_else(|| "stop".to_string()),
    })
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
        "Usage: copilot-stop-hook --transcript-path <PATH> [--store-root <PATH>] --workspace-slug <SLUG> [--trigger <NAME>]"
    );
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use std::path::PathBuf;

    use super::resolve_store_root;

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
}