# [Board] Integrate Draftboard State into `next` and `status` Commands

## Purpose

Make the existing `ticket next` and `ticket status` commands draftboard-aware so that agents receive board context automatically, without needing to call `board show` separately. This is the integration point where the draftboard becomes part of the standard agent workflow rather than an opt-in sidecar.

## Component Boundaries

### In scope
- Modify `cmd_next()` in `tools/cli/ticket-cli/src/cli/commands/ops.rs` to:
  - Subtract currently board-checked-in tickets from the candidate list
  - Warn when WIP limit is reached or nearly reached
  - Include board summary in the `--json` output envelope
- Modify `cmd_status()` to include a board summary section
- Modify `cmd_ready_overview()` to include board WIP context
- Modify `mcp_ticket-mcp_next_tickets` tool to include board summary in response
- Add `--include-board` / `--no-board` flag to `next` and `status` for opt-in/opt-out

### Out of scope
- Board storage logic (owned by `0db86ac1`)
- Board CLI subcommands (owned by `bcc111c6`)
- Board MCP tools (owned by `ec52f7cb`)

## Behavioral Changes

### `ticket next` (enhanced)

Before this change:
```json
{
  "command": "next",
  "items": [
    { "id": "854f0e8f", "title": "...", "priority": "high", "state": "ready" },
    { "id": "34bc4938", "title": "...", "priority": "high", "state": "in-implementation" }
  ]
}
```

After this change:
```json
{
  "command": "next",
  "board": {
    "active_count": 3,
    "max_wip": 5,
    "stale_count": 1,
    "wip_limit_reached": false,
    "checked_in_tickets": ["34bc4938"]
  },
  "items": [
    { "id": "854f0e8f", "title": "...", "priority": "high", "state": "ready" }
  ],
  "excluded_by_board": [
    { "id": "34bc4938", "title": "...", "agent": "copilot-session-2", "reason": "checked_in" }
  ]
}
```

Key differences:
- `34bc4938` is excluded from `items` because another agent is checked in
- `board` summary included for WIP awareness
- `excluded_by_board` section explains why items were filtered

### `ticket status` (enhanced)

New section in status output:

```
Board: [3/5 active] [1 stale ⚠]
  Checked in: 854f0e8f (copilot-session-1), 34bc4938 (copilot-session-2), 8c185de3 ⚠ (copilot-session-3, stale)
```

### WIP limit warning in `next`

When `active_count >= max_wip`:
```
⚠ WIP limit reached (5/5). Check out a ticket before starting new work.
  Review stale or completed entries with the user before using `ticket board clean`.
```

When `active_count >= max_wip - 1`:
```
ℹ WIP near limit (4/5). One slot remaining.
```

### Stale-entry escalation in `next` and `status`

When an entry has exceeded the one-hour TTL, `next` and `status` should surface it as a high-priority human-review item rather than treating it as silently stale. The output should guide the operator toward one of two explicit actions:

- renew the entry via `board heartbeat` / `board show --agent <AGENT_ID>` if the session is still active
- clean the entry explicitly after user review if the session is abandoned

### MCP `next_tickets` enhancement

The `mcp_ticket-mcp_next_tickets` response includes the same `board` summary and `excluded_by_board` sections, so that agent sessions calling `next_tickets` get the full picture without a separate `board_show` call.

## Design Notes

### Opt-out

The `--no-board` flag disables board filtering in `next` and `status` for cases where the operator wants to see all candidates regardless of board state. Default is `--include-board` (board-aware).

### Performance

`board_show()` is a single redb read transaction. Adding it to `next` and `status` adds one read transaction per call. Acceptable for the expected call frequency (once per agent session startup, not hot-path).

### Backward Compatibility

- JSON output gains new fields (`board`, `excluded_by_board`) but does not remove existing fields.
- `items` array semantics change: previously included all unblocked tickets; now excludes board-checked-in tickets by default. The `--no-board` flag restores the old behavior.
- Human-readable output gains a board summary line but is otherwise unchanged.

## Acceptance Criteria

- [ ] `ticket next` excludes board-checked-in tickets from the candidate list by default
- [ ] `ticket next --json` includes `board` summary and `excluded_by_board` sections
- [ ] `ticket next --no-board` restores pre-board behavior (all unblocked candidates)
- [ ] `ticket next` warns when WIP limit is reached or near-limit
- [ ] `ticket status` includes a board summary section with active entries and stale warnings
- [ ] `mcp_ticket-mcp_next_tickets` includes board context in the response
- [ ] Board-checked-in tickets still appear in `status` (they're active work), just excluded from `next` (not available for new agents)
- [ ] Entries older than the one-hour TTL are surfaced as high-priority human-review items in both `next` and `status`
- [ ] Performance: board read adds < 5ms overhead to `next` and `status`
- [ ] Backward compatibility: existing JSON consumers are not broken by new fields
- [ ] Integration test: check-in a ticket → verify `next` excludes it → check-out → verify `next` includes it again
