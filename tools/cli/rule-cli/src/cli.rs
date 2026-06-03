use std::ffi::OsString;

use clap::Parser;
use serde_json::{
    Value,
    json,
};

mod args;
mod dispatch;
mod helpers;
mod importing;
mod rendering;
#[cfg(test)]
mod tests;

pub use args::*;

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("rule error: {0}")]
    Rule(#[from] rule_api::error::RuleError),
    #[error("target config error: {0}")]
    TargetConfig(#[from] rule_api::TargetConfigError),
    #[error("storage error: {0}")]
    Storage(#[from] memory_api::error::StorageError),
    #[error("{0}")]
    BadRequest(String),
}

pub enum CliOutput {
    Machine(Value, MachineOutputFormat),
    Text(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineOutputFormat {
    Json,
    Toon,
}

pub fn run(cli: RuleCli) -> Result<CliOutput, CliRunError> {
    let index_root = helpers::resolve_index_root(
        cli.index_root.as_deref(),
        cli.workspace_root.as_deref(),
    );
    let payload = dispatch::dispatch_with_workspace_root(
        cli.command,
        &index_root,
        cli.workspace_root.as_deref(),
    )?;
    if let Some(format) = machine_output_format(cli.json, cli.toon) {
        Ok(CliOutput::Machine(payload, format))
    } else {
        Ok(CliOutput::Text(
            serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| format!("{payload:?}")),
        ))
    }
}

pub fn error_output(
    message: &str,
    format: Option<MachineOutputFormat>,
) -> String {
    let payload = json!({"status": "error", "message": message});
    match format {
        Some(MachineOutputFormat::Json) => payload.to_string(),
        Some(MachineOutputFormat::Toon) =>
            toon_format::encode_default(&payload)
                .unwrap_or_else(|_| format!("status: error\nmessage: {message}")),
        None => message.to_string(),
    }
}

pub fn render_machine_output(
    payload: &Value,
    format: MachineOutputFormat,
) -> Result<String, String> {
    match format {
        MachineOutputFormat::Json =>
            serde_json::to_string_pretty(payload).map_err(|err| err.to_string()),
        MachineOutputFormat::Toon =>
            toon_format::encode_default(payload).map_err(|err| err.to_string()),
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

pub fn parse_cli_from<I, T>(args: I) -> Result<RuleCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    RuleCli::try_parse_from(args)
}
