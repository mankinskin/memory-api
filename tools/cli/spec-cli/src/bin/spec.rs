use clap::error::ErrorKind;

use spec_cli::cli::{
    CliOutput,
    error_output,
    parse_cli_from,
    render_machine_output,
    requested_machine_output_format_from_args,
    run,
};

fn main() {
    let cli = match parse_cli_from(std::env::args_os()) {
        Ok(cli) => cli,
        Err(err) => {
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | ErrorKind::DisplayVersion
            ) {
                print!("{err}");
                std::process::exit(0);
            }
            let rendered = error_output(
                &err.to_string(),
                requested_machine_output_format_from_args(),
            );
            eprintln!("{rendered}");
            std::process::exit(2);
        },
    };

    match run(cli) {
        Ok(CliOutput::Machine(value, format)) => {
            let exit_code = validate_links_exit_code(&value);
            match render_machine_output(&value, format) {
                Ok(rendered) => {
                    println!("{rendered}");
                    if exit_code != 0 {
                        std::process::exit(exit_code);
                    }
                },
                Err(err) => {
                    eprintln!("{}", error_output(&err, Some(format)));
                    std::process::exit(1);
                },
            }
        },
        Ok(CliOutput::Text(text)) => {
            let exit_code = validate_links_exit_code_from_text_payload(&text);
            println!("{text}");
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        },
        Err(err) => {
            eprintln!(
                "{}",
                error_output(
                    &err.to_string(),
                    requested_machine_output_format_from_args(),
                )
            );
            std::process::exit(1);
        },
    }
}

/// `validate-links` reports findings without treating them as errors, so its
/// non-zero exit code is decided here rather than via `Result::Err`.
fn validate_links_exit_code(payload: &serde_json::Value) -> i32 {
    if payload.get("command").and_then(|v| v.as_str()) == Some("validate_links")
        && payload.get("valid").and_then(|v| v.as_bool()) == Some(false)
    {
        1
    } else {
        0
    }
}

fn validate_links_exit_code_from_text_payload(text: &str) -> i32 {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => validate_links_exit_code(&value),
        Err(_) => 0,
    }
}
