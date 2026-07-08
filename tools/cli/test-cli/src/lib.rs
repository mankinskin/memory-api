mod args;
mod handlers;

use clap::Parser;
use serde_json::{
    json,
    Value,
};

use log_api::{
    LogError,
    LogStoreConfig,
};
use memory_api::workspace;
use test_api::{
    TestError,
    TestStoreConfig,
};

pub use args::*;

use handlers::{
    dispatch_read_queries,
    dispatch_recording,
    dispatch_reporting,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineOutputFormat {
    Json,
    Toon,
}

pub enum CliOutput {
    Machine(Value, MachineOutputFormat),
    Text(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("test error: {0}")]
    Test(#[from] TestError),
    #[error("log error: {0}")]
    Log(#[from] LogError),
    #[error("invalid timestamp '{0}': {1}")]
    Timestamp(String, String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("io error at {0}: {1}")]
    Io(String, String),
    #[error("failed to launch command '{0}': {1}")]
    Spawn(String, String),
}

pub fn run(cli: TestCli) -> Result<CliOutput, CliRunError> {
    if matches!(
        cli.command,
        TestCommand::RecordSpec(_)
            | TestCommand::Record(_)
            | TestCommand::LogRecord(_)
            | TestCommand::Run(_)
    ) && cli.store_root.is_none()
        && cli.workspace_root.is_none()
    {
        return Err(CliRunError::BadRequest(
            "entity creation requires explicit --workspace <path> or --store-root <path>"
                .to_string(),
        ));
    }

    let store_root = workspace::resolve_requested_store_root(
        cli.store_root.as_deref(),
        cli.workspace_root.as_deref(),
        None,
        TEST_STORE_DIR,
    );
    let config = TestStoreConfig::new(store_root.clone(), cli.workspace_slug.clone());

    let log_root = match store_root.parent() {
        Some(parent) => parent.join(LOG_STORE_DIR),
        None => workspace::resolve_requested_store_root(
            None,
            cli.workspace_root.as_deref(),
            None,
            LOG_STORE_DIR,
        ),
    };
    let log_config = LogStoreConfig::new(log_root, cli.workspace_slug.clone());

    let payload = dispatch(&config, &log_config, cli.command)?;

    match machine_output_format(cli.json, cli.toon) {
        Some(format) => Ok(CliOutput::Machine(payload, format)),
        None => Ok(CliOutput::Text(render_human(&payload))),
    }
}

fn dispatch(
    config: &TestStoreConfig,
    log_config: &LogStoreConfig,
    command: TestCommand,
) -> Result<Value, CliRunError> {
    match command {
        TestCommand::RecordSpec(_)
        | TestCommand::Record(_)
        | TestCommand::LogRecord(_)
        | TestCommand::Run(_) => dispatch_recording(config, log_config, command),
        TestCommand::GetSpec(_)
        | TestCommand::Get(_)
        | TestCommand::ListSpecs
        | TestCommand::List(_)
        | TestCommand::Logs(_) => dispatch_read_queries(config, log_config, command),
        TestCommand::StoreIndex
        | TestCommand::Benchmarks(_)
        | TestCommand::Summary
        | TestCommand::Audit => dispatch_reporting(config, command),
    }
}

fn render_human(payload: &Value) -> String {
    serde_json::to_string_pretty(payload).unwrap_or_else(|_| format!("{payload:?}"))
}

pub fn error_output(
    message: &str,
    format: Option<MachineOutputFormat>,
) -> String {
    let payload = json!({"status": "error", "message": message});
    match format {
        Some(MachineOutputFormat::Json) => payload.to_string(),
        Some(MachineOutputFormat::Toon) => toon_format::encode_default(&payload)
            .unwrap_or_else(|_| format!("status: error\nmessage: {message}")),
        None => message.to_string(),
    }
}

pub fn render_machine_output(
    payload: &Value,
    format: MachineOutputFormat,
) -> Result<String, String> {
    match format {
        MachineOutputFormat::Json => {
            serde_json::to_string_pretty(payload).map_err(|err| err.to_string())
        },
        MachineOutputFormat::Toon => {
            toon_format::encode_default(payload).map_err(|err| err.to_string())
        },
    }
}

pub fn machine_output_format(
    as_json: bool,
    as_toon: bool,
) -> Option<MachineOutputFormat> {
    if as_json {
        Some(MachineOutputFormat::Json)
    } else if as_toon {
        Some(MachineOutputFormat::Toon)
    } else {
        None
    }
}

pub fn requested_machine_output_format_from_args() -> Option<MachineOutputFormat> {
    machine_output_format(
        std::env::args().any(|arg| arg == "--json"),
        std::env::args().any(|arg| arg == "--toon"),
    )
}

pub fn parse_cli_from<I, T>(args: I) -> Result<TestCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    TestCli::try_parse_from(args)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod lib_tests;
