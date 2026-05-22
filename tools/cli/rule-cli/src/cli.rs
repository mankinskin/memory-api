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
    Json(Value),
    Text(String),
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
    if cli.json {
        Ok(CliOutput::Json(payload))
    } else {
        Ok(CliOutput::Text(
            serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| format!("{payload:?}")),
        ))
    }
}

pub fn error_output(
    message: &str,
    as_json: bool,
) -> String {
    if as_json {
        json!({"status": "error", "message": message}).to_string()
    } else {
        message.to_string()
    }
}

pub fn parse_cli_from<I, T>(args: I) -> Result<RuleCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    RuleCli::try_parse_from(args)
}
