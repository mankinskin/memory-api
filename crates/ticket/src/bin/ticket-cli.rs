use clap::error::ErrorKind;
use memory_kernel::runtime::init_transport_tracing;
use std::io::{self, Write};

use ticket::cli::{
    CliOutput,
    error_output,
    parse_cli_from,
    render_machine_output,
    requested_machine_output_format_from_args,
    run,
};

fn main() {
    init_transport_tracing("ticket_cli=info", None, None, "warn");

    let cli = match parse_cli_from(std::env::args_os()) {
        Ok(cli) => cli,
        Err(err) => {
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | ErrorKind::DisplayVersion
            ) {
                finish_stdout(&err.to_string(), 0);
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
                    finish_stdout(&rendered, exit_code);
                },
                Err(err) => {
                    eprintln!("{}", error_output(&err, Some(format)));
                    std::process::exit(1);
                },
            }
        },
        Ok(CliOutput::Text(text)) => finish_stdout(&text, 0),
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

#[derive(Debug, Eq, PartialEq)]
enum StdoutWrite {
    Written,
    BrokenPipe,
}

fn finish_stdout(rendered: &str, exit_code: i32) -> ! {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    match write_stdout(&mut stdout, rendered) {
        Ok(StdoutWrite::Written) => std::process::exit(exit_code),
        Ok(StdoutWrite::BrokenPipe) => std::process::exit(0),
        Err(err) => {
            eprintln!("failed to write stdout: {err}");
            std::process::exit(1);
        },
    }
}

fn write_stdout<W: Write>(
    writer: &mut W,
    rendered: &str,
) -> io::Result<StdoutWrite> {
    match writeln!(writer, "{rendered}") {
        Ok(()) => Ok(StdoutWrite::Written),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {
            Ok(StdoutWrite::BrokenPipe)
        },
        Err(err) => Err(err),
    }
}

/// `validate-links` reports findings without treating them as errors, so its
/// non-zero exit code is decided here rather than via `Result::Err`. The
/// envelope nests the actual command payload under `"payload"`.
fn validate_links_exit_code(envelope: &serde_json::Value) -> i32 {
    let payload = envelope.get("payload").unwrap_or(envelope);
    if payload.get("command").and_then(|v| v.as_str()) == Some("validate_links")
        && payload.get("valid").and_then(|v| v.as_bool()) == Some(false)
    {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use super::{StdoutWrite, write_stdout};

    struct BrokenPipeWriter;

    impl Write for BrokenPipeWriter {
        fn write(
            &mut self,
            _: &[u8],
        ) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn broken_pipe_stdout_is_clean_termination() {
        let mut writer = BrokenPipeWriter;

        let outcome = write_stdout(&mut writer, "output")
            .expect("broken pipes should not be propagated as failures");

        assert_eq!(outcome, StdoutWrite::BrokenPipe);
    }
}
