# Ticket-domain transport workspace-resolution parity (focused first run)

The first parity run. Establishes the shared-resolver adoption + pure-transport audit pattern that the spec/rule/audit domains will reuse.

## Problem

`ticket-mcp` required a `workspace` argument for `get_ticket` and `get_ticket_description`. It resolved only that store before prefix lookup, so a caller had to redundantly choose a workspace for a read and could not tell which aggregate roots had been searched when a prefix failed.

## Implemented behavior

- Read-by-ID MCP inputs now allow an omitted workspace and default to the server aggregate root.
- An explicitly supplied workspace is still opened as the requested root, retaining its indexed descendant scan roots for the lookup.
- Prefix misses report the scan-root paths searched.
- Creation remains explicitly scoped by its required workspace input.

## Acceptance criteria

1. `get_ticket` and `get_ticket_description` accept `{ "id": "..." }` without a workspace parameter and resolve tickets present in the configured aggregate root.
2. Supplying a parent workspace/root still resolves tickets in an indexed descendant store.
3. A missing ID/prefix error reports all workspace roots searched.
4. Creation remains explicitly scoped by required `workspace` input.
5. Unit tests cover omitted workspace, descendant lookup, and searched-root diagnostics.

## Validation

- `cargo test -p ticket-mcp` — passed, 18 tests.
- VS Code diagnostics — no errors in the three modified ticket-MCP files.
- `git diff --check` — passed.

## Review blocker

The ticket remains `in-implementation` until dependency `ef0ebf38-7f55-4bd7-bf0c-0b416650ee0b` progresses from `in-implementation`; the ticket lifecycle correctly prevents transition to `in-review` before then.

## Related spec

- `3fd3aaff-b0d9-494b-8bbb-802d71140d59` — ticket-mcp read workspace resolution.

## Depends on

- `ef0ebf38-7f55-4bd7-bf0c-0b416650ee0b` — shared memory-api descendant-discovery helper (CLI groundwork) is reused here for mcp/http.