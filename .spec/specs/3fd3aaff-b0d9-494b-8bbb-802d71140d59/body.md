# Summary

Ticket-MCP ticket-ID read operations resolve against the configured aggregate ticket root by default, including indexed descendant stores, while creation remains explicitly rooted.

## Requirements

- `get_ticket` and `get_ticket_description` accept an omitted workspace selector.
- An omitted selector resolves to the server index root.
- A supplied selector resolves that store and its indexed descendants.
- No-match diagnostics report every root searched.
- Create inputs retain required workspace selectors.

## Implementation

`TicketRefInput.workspace` is optional only for read-by-ID operations. `TicketServer::resolve_uuid_for_read` resolves prefixes through the selected aggregate `TicketStore` and appends persisted scan-root paths to no-match diagnostics.

## Validation

- `cargo test -p ticket-mcp` passed: 18 tests.
- VS Code diagnostics reported no errors in the modified Rust sources.
- `git diff --check` passed.

## Related Implementation Ticket

- `27558fde-37b0-43eb-86c6-cfbe2d99a0b8`.