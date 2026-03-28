---
description: "Use when editing ticket system crates/tools. Covers state transitions, storage/search boundaries, and workflow expectations."
applyTo: "**"
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

## Ticket Quality — Standing Obligations

These rules apply during **every session**, not only when working on ticket-system code.

### Orientation (start of every session)

Before writing any code, run a quick orientation to understand the current ticket landscape:

```bash
# Survey all new tickets
./target/debug/ticket.exe list --where state=new --json

# Check for stale in-implementation tickets that may conflict with your work
./target/debug/ticket.exe list --where state=in-implementation --json

# Check overall graph health
./target/debug/ticket.exe health --all --json
```

Alternatively, use the MCP ticket tools (`mcp_ticket-mcp_list_tickets`, `mcp_ticket-mcp_health`) when the MCP server is running.

### Discovery Before Creating

Always search for existing tickets before creating new ones. Duplicate tickets degrade store quality.

```bash
./target/debug/ticket.exe search "<keywords>" --json
```

Or via MCP: `mcp_ticket-mcp_list_tickets` with a `where` filter, or `mcp_ticket-mcp_get_ticket_description`.

### Continuous State Updates

Update ticket state immediately when the work status changes — do not defer to the end of a session:

| Situation | Action |
|---|---|
| Starting refinement on a ticket | `update --state in-refinement` |
| Starting implementation | `update --state in-implementation` |
| All acceptance criteria met | `close <id>` |
| Ticket is no longer relevant | `cancel <id>` with a reason |

### Dependency Maintenance

After completing significant work, check whether finished tickets unblock others and update those links:

```bash
# Find what a completed ticket blocks
./target/debug/ticket.exe topgraph <id> --json \
  | jq -r '.payload.nodes[] | select(.state=="new" or .state=="ready") | .id'
```

Add missing `depends_on` edges when you discover undocumented dependencies. Use `--reason` on every link to explain *why* the dependency exists.

### Commit Checkpoint Suggestions

Suggest a `git commit` checkpoint to the user when any of the following is true:

- A ticket transitions to `closed` (work milestone reached).
- A batch of related tickets all reach `closed` or `in-implementation` together.
- A dependency graph changes materially (new links added/removed).
- A tracked bug is fixed and its ticket closed.

Phrase suggestions like:

> "Ticket `<title>` is now closed — good checkpoint for a commit. Suggested message: `<imperative summary of what was done>`."

### Aggressive Quality Improvement

Opportunistically improve ticket quality whenever you touch the store:

- Fill in missing `description`, `priority`, or `type` fields on tickets you encounter.
- Split vague tickets into concrete, actionable child tickets linked with `depends_on`.
- Remove or merge duplicate tickets.
- Verify that `in-implementation` tickets actually have an active owner/context; flag stale ones.
- After any structural refactor, re-run `ticket health --all` and resolve reported issues.

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

# Health-check all new tickets (--where filter)
ticket health --all --where state=new --json

# Health-check a subgraph, filtering to a specific type
ticket health abcd1234 --where type=tracker-improvement --json
```

### Command Chaining (pipe via --stdin)

```bash
# List tickets → pipe IDs → health check
ticket list --where priority=high --json \
  | jq -r '.payload.items[].id' \
  | ticket health --stdin --json

# Subgraph → filter new tickets → health check
ticket subgraph abcd1234 --json \
  | jq -r '.payload.nodes[] | select(.state=="new") | .id' \
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
