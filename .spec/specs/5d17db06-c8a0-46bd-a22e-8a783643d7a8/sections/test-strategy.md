## Current Test Coverage

The extension has unit tests only — no integration or e2e activation tests.

| File | What it tests |
|---|---|
| `test/unit/buildStateGroups.test.ts` | `buildStateGroups` tree logic: state grouping, root-ticket filtering by same-state parent, schema state ordering, unknown state alphabetic fallback |

Tests are run with: `npm run test:unit` (Jest + ts-jest; no browser required).

## Testing Strategy

### Unit Tests (existing)

- Pure-function logic extracted from `TicketTreeProvider` (grouping, dependency maps).
- Mock `TicketSummary` arrays and `EdgeRecord` arrays as fixtures.
- No VS Code API dependencies in test code.

### Integration Tests (recommended additions)

1. **Server lifecycle**: spawn a real `ticket-viewer` process with a temp workspace; assert `TICKET_VIEWER_PORT=N` is parsed and the extension connects.
2. **Tree population**: after server is reachable, assert that `TicketTreeProvider.allTickets` is non-empty and state groups match `GET /api/schema` order.
3. **Mutation round-trip**: `createTicket` → assert appears in tree; `closeTicket` → assert state changes to `done`.

Integration tests require the `@vscode/test-electron` runner.

## Known Gaps

- No test for `BrowserBridge` HTTP control server routes.
- No test for `resolveActiveWorkspace` multi-folder QuickPick logic.
- No test for `startServerTask` port-parsing via stdout `TICKET_VIEWER_PORT=N`.
- `setState` command uses a hard-coded `TICKET_STATES` list rather than the live schema; a test confirming schema-driven state orders would catch drift.
