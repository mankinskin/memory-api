use super::*;

pub(super) fn domain_names() -> Vec<&'static str> {
    domains().into_iter().map(|d| d.domain()).collect()
}

fn in_process_supported(
    domain: &str,
    operation: &str,
) -> bool {
    match domain {
        "ticket" | "spec" | "rule" => OPERATIONS.contains(&operation),
        "audit" => ["search", "scan"].contains(&operation),
        "session" => ["create", "get", "search", "update"].contains(&operation),
        "test" => ["create", "get", "search", "update"].contains(&operation),
        "log" => ["create", "get", "search", "update"].contains(&operation),
        "doc" => false,
        _ => false,
    }
}

fn cli_supported(
    domain: &str,
    operation: &str,
) -> bool {
    ["ticket", "spec", "rule"].contains(&domain) && operation != "move"
}

fn http_supported(
    domain: &str,
    operation: &str,
) -> bool {
    domain == "ticket" && ["get", "search"].contains(&operation)
}

pub(super) fn is_supported(
    domain: &str,
    transport: &str,
    operation: &str,
) -> bool {
    match transport {
        "in-process" => in_process_supported(domain, operation),
        "cli" => cli_supported(domain, operation),
        "http" => http_supported(domain, operation),
        "mcp" => match domain {
            "ticket" => ["create", "get", "search", "update", "delete"]
                .contains(&operation),
            "spec" => ["create", "get", "search", "update", "delete", "scan"]
                .contains(&operation),
            "rule" => ["create", "get", "search", "update", "scan"]
                .contains(&operation),
            _ => false,
        },
        _ => false,
    }
}

pub(super) fn expected_blocked_reason(
    domain: &str,
    transport: &str,
    operation: &str,
) -> String {
    match transport {
        "in-process" =>
            if operation == "move" {
                format!(
                    "{domain} move surface is not adapter-backed in memory-matrix yet"
                )
            } else {
                unsupported(operation, domain)
            },
        "cli" =>
            if operation == "move" {
                format!(
                    "cli transport for domain `{domain}` operation `move` is not wired in memory-matrix yet; in-process move cells exercise the adapter-backed move kernel"
                )
            } else {
                format!(
                    "cli transport for domain `{domain}` operation `{operation}` is not wired yet"
                )
            },
        "http" =>
            if domain == "ticket" {
                format!(
                    "http transport for domain `ticket` operation `{operation}` is not wired yet; currently only `ticket.get@http` and `ticket.search@http` are exercised through the ticket-http router surface"
                )
            } else {
                format!(
                    "http transport for domain `{domain}` operation `{operation}` is not wired yet"
                )
            },
        _ => format!(
            "transport `{transport}` for domain `{domain}` operation `{operation}` is not wired in the matrix harness yet; recorded as blocked-with-reason per real-transport rollout"
        ),
    }
}
