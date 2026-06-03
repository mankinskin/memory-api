use std::{
    env,
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
    let config = SessionStoreConfig::new(args.store_root, args.workspace_slug);

    config.capture_copilot_transcript(args.transcript_path, args.trigger)?;
    Ok(())
}

struct Args {
    transcript_path: PathBuf,
    store_root: PathBuf,
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
        store_root: store_root.ok_or_else(|| {
            SessionError::InvalidHookInput("missing --store-root".to_string())
        })?,
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
        "Usage: copilot-stop-hook --transcript-path <PATH> --store-root <PATH> --workspace-slug <SLUG> [--trigger <NAME>]"
    );
}