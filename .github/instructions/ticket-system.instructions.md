---
description: "Use when editing ticket system crates/tools. Covers state transitions, storage/search boundaries, and workflow expectations."
applyTo: "crates/ticket-api/**,tools/ticket-*/**"
---

# Ticket System Guidance

## Scope

Applies to:
- `crates/ticket-api/`
- `tools/ticket-cli/`
- `tools/ticket-http/`
- `tools/ticket-mcp/`
- `tools/ticket-viewer/`

## Design Constraints

- Respect ticket lifecycle/state machine invariants.
- Keep storage/index behavior backward compatible unless explicitly requested.
- Preserve clear separation between API, storage, transport, and UI layers.

## Workflow Expectations

- For larger ticket-system work, start with ticket planning workflow first.
- Search existing tickets/issues before introducing new behavior.
- Record acceptance criteria for behavior changes.

## CLI Examples

### Health Checks

```bash
# Health-check a subgraph rooted at a ticket (BFS traversal)
ticket health abcd1234 --json

# Health-check all tickets
ticket health --all --json

# Health-check all open tickets (--where filter)
ticket health --all --where state=open --json

# Health-check a subgraph, filtering to a specific type
ticket health abcd1234 --where type=tracker-improvement --json
```

### Command Chaining (pipe via --stdin)

```bash
# List tickets → pipe IDs → health check
ticket list --where priority=high --json \
  | jq -r '.payload.items[].id' \
  | ticket health --stdin --json

# Subgraph → filter open tickets → health check
ticket subgraph abcd1234 --json \
  | jq -r '.payload.nodes[] | select(.state=="open") | .id' \
  | ticket health --stdin --json

# Topgraph → health check all reverse dependencies
ticket topgraph abcd1234 --json \
  | jq -r '.payload.nodes[].id' \
  | ticket health --stdin --json
```

### Batch (CLI-syntax, transactional)

`ticket batch` reads one CLI command per line from stdin (or `--file`). All
commands execute against the same store in order. If any command fails, all
prior writes are rolled back automatically. Blank lines and `#` comments are
ignored.

```bash
# Heredoc — create tickets + link, all atomic
ticket batch --json <<'EOF'
create --title "Extract GPU pipeline" --type tracker-improvement
create --title "Add shader cache" --type tracker-improvement
# link is resolved after creates succeed
link --from <UUID-A> --to <UUID-B> --kind depends_on
EOF

# From a checked-in batch file
ticket batch --file scripts/bootstrap-tickets.txt --json

# Stdin from another process
echo -e "create --title 'Setup CI' --type tracker-improvement\nclose <UUID>" \
  | ticket batch --json
```

**Rules:**
- Each line is parsed identically to a top-level `ticket <subcommand>` call.
- `serve`, `watch`, nested `batch`, `scan`, lease commands, and
  config commands (`add-root`, `workspace`, `export-command-schema`) are
  rejected with a clear error — they cannot be used inside a batch.
- On rollback: `create` → deleted, `update` → state/fields restored, `link`
  → edge removed. Read-only commands (`get`, `list`, `search`, `health`, etc.)
  produce no undo entry and are not affected by rollback.
- No `--index-root` requirement — uses normal workspace resolution like any
  other CLI command.

## HTTP Endpoints

```
GET /api/graph/subgraph?workspace=default&root=<UUID>&depth=2
GET /api/graph/topgraph?workspace=default&root=<UUID>&depth=2
GET /api/graph/health?workspace=default&all=true
GET /api/graph/health?workspace=default&root=<UUID>&depth=4&direction=out
```

## Validation

- Prefer focused tests for changed modules before broader suites.
- Verify search/index behavior when touching ticket query paths.
- Confirm no regressions in CLI or MCP-facing flows for changed endpoints.
