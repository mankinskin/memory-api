# ticket-cli

CLI interface for `ticket-api`.

This tool provides the `ticket` command for local ticket management, workflow automation, and serving the ticket HTTP API.

## Build and run

```bash
cargo build -p ticket-cli --bin ticket
cargo run -p ticket-cli --bin ticket -- --help
```

## Global options

- `--json`: emit machine-readable JSON envelopes
- `--request-id <id>`: include request id in JSON output
- `--index-root <path>`: override index root directory
- `--schema-dir <path>`: load additional ticket schema TOML files

## Core commands

- `create`: create a new ticket
- `get`: read one ticket by UUID
- `update`: patch fields and/or transition state
- `list`: list tickets with optional filters
- `delete`: soft-delete a ticket
- `search`, `query`: full-text + metadata search
- `scan`: run index reconciliation over scan roots

## Workflow commands

- `claim`, `unclaim`, `leases`: lease management for active work
- `link`, `unlink`, `links`: dependency graph edge management
- `status`: state summary, ready tickets, and parallel opportunities
- `ready-overview`: JSON summary of ready tickets
- `workspace`: manage named workspaces
- `watch`: watch scan roots and auto-reconcile changes

## Protocol / automation commands

- `exec`: run one JSON command request (stdin)
- `batch`: run multiple JSON command requests atomically (stdin or file)
- `export-command-schema`: output supported command namespace/schema

## Repro tracking

- `repro`: append reproduction metadata (outcome, commit, command, note, timestamp)

## HTTP server mode

`ticket-cli` can start the ticket HTTP API directly:

```bash
ticket serve --host 127.0.0.1 --port 4000 --workspace default
```

This serves the same API surface used by `ticket-http` and `ticket-mcp`.

## Examples

```bash
# Create
ticket create --title "Bug: auth fails on retry" --state open \
  --field component=ticket-cli --field risk_level=medium

# Start work
ticket claim --id <uuid> --agent copilot
ticket update --id <uuid> --to-state in-progress

# Link a dependency
ticket link --from <uuid-a> --to <uuid-b> --kind depends_on --reason "needs API contract"

# Search
ticket search "auth retry" --limit 20

# Serve HTTP API
ticket serve --port 4000
```
