<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=71ffe450-aad5-4c26-af4f-d5d4cfcd8d8f slug=memory-api/readme/tools/cli/ticket-cli/l1 -->
# ticket-cli

CLI interface for `ticket-api`.

## Interface

Use `ticket` for local ticket CRUD, dependency graphs, ready-work discovery, board coordination, and JSON automation.

- `create`, `get`, `update`, `close`, `cancel`, `delete`, `list`, `search`: maintain tickets and state transitions.
- `link`, `unlink`, `links`, `subgraph`, `topgraph`: inspect and manage dependency edges.
- `status`, `ready-overview`, `next`: discover unblocked work.
- `board ...`: inspect and coordinate active work on the draft board.
- `exec`, `batch`, `export-command-schema`: drive the command surface from automation.
- `serve`: expose the ticket HTTP API directly from the CLI.

Global options:

- `--json`: emit machine-readable JSON output.
- `--request-id <id>`: include a request id in JSON envelopes.
- `--index-root <path>`: override the `.ticket` index root.
- `--workspace-root <path>`: target a nested workspace repo root and normalize it to the owning `.ticket` store.
- `--schema-dir <path>`: load additional ticket schema files.

## Usage

Build or run from a checkout of `memory-viewers/memory-api`:

```bash
cargo build -p ticket-cli --bin ticket
cargo run -p ticket-cli --bin ticket -- --help
```

`ticket` discovers the nearest `.ticket` workspace by walking up from the current directory. Use `--index-root` when you need to point at another ticket store, or `--workspace-root` when you want to target a nested workspace repo root from an ancestor checkout.

## Examples

```bash
# Inspect the current board state
ticket board show

# Move one ticket forward
ticket update --id <ticket-id> --to-state in-progress

# Inspect dependency context before starting work
ticket subgraph <ticket-id>

# Target a nested workspace from the repo root
ticket --workspace-root memory-viewers/memory-api get <ticket-id>

# Serve the HTTP API locally
ticket serve --host 127.0.0.1 --port 4000
```
