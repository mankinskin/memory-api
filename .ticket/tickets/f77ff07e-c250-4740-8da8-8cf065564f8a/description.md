# Plan: State Fast-Forward (`ticket close` / `ticket cancel`)

## Problem

The tracker-improvement schema has 11 states with a strict transition chain:
`open → in-progress → review → validating → validated → release-candidate → released → monitoring → done`

To close a ticket, agents must issue 8 sequential `ticket update --to-state <s>` commands.
This is the **#1 source of boilerplate** in agent workflows — during the migration session,
every ticket closure required a loop or a dedicated Python script.

## Proposed Solution

### Option A: `ticket close <id>` command
- Traverses all intermediate states automatically to reach `done`
- Internally calls the same state machine transitions (preserving history)
- Fails if any intermediate transition is blocked

### Option B: `--skip-intermediate` flag on `update`
- `ticket update --id <id> --to-state done --skip-intermediate`
- Same behavior, no new subcommand

### `ticket cancel <id>`
- Direct transition to `cancelled` from any state
- The schema already allows `cancelled` as a target from most states
- If not all states have a transition to `cancelled`, add them

## Evidence

During the migration session, this pattern appeared ~15 times:
```python
for state in ["in-progress", "review", "validating", "validated",
              "release-candidate", "released", "monitoring", "done"]:
    ticket update --id <id> --to-state <state>
```

## MCP Consideration

The MCP server should also expose a `close_ticket` or `cancel_ticket` tool,
or accept a `skip_intermediate: true` parameter on updates.
