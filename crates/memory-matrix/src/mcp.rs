use super::*;
use mcp_rule::dispatch_rule_mcp;
use mcp_spec::dispatch_spec_mcp;
use mcp_ticket::{
    dispatch_ticket_mcp,
    ensure_status_ok,
    extract_mcp_json,
};

#[path = "mcp/mcp_rule.rs"]
mod mcp_rule;
#[path = "mcp/mcp_spec.rs"]
mod mcp_spec;
#[path = "mcp/mcp_ticket.rs"]
mod mcp_ticket;
#[path = "mcp/stdio_sentinel.rs"]
mod stdio_sentinel;
#[cfg(test)]
#[path = "mcp/mcp_tests.rs"]
mod tests;
use stdio_sentinel::dispatch_ticket_mcp_stdio_sentinel_get;
#[cfg(test)]
use stdio_sentinel::{
    STDIO_TAIL_BYTES,
    build_failure_bundle,
    classify_stdio_read_error,
    extract_stdio_tool_json,
    tail_from_bytes,
    validate_sentinel_ticket_id,
};

pub(super) fn dispatch_mcp_subprocess_failure_probe(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
    metadata: Option<&DispatchMetadata>,
) -> CellResult {
    stdio_sentinel::dispatch_mcp_subprocess_failure_probe(
        domain, operation, ctx, metadata,
    )
}

pub(super) fn dispatch_mcp(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
    metadata: Option<&DispatchMetadata>,
) -> CellResult {
    match domain {
        "ticket" => dispatch_ticket_mcp(operation, ctx, metadata),
        "spec" => dispatch_spec_mcp(operation, ctx),
        "rule" => dispatch_rule_mcp(operation, ctx),
        _ => blocked(format!(
            "mcp transport for domain `{domain}` operation `{operation}` is not wired yet"
        )),
    }
}
