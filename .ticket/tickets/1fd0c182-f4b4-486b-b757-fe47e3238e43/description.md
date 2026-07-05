# Rule-domain transport workspace-resolution parity

Follow-up after the ticket-domain first run. Adopt the shared memory-api resolver + pure-transport pattern for the rule transports.

## Scope
- rule-cli + rule-mcp + rule-http consume the shared memory-api workspace resolver; nested `.rule` discovery is recursive from a parent workspace.
- Per `ef0ebf38`, rule-cli was moved off its private descendant-traversal logic — verify it fully delegates to the shared resolver and bring rule-mcp/http to parity.
- No per-transport resolution logic remains in any rule transport.

## Acceptance criteria (test-validatable)
1. From a parent workspace, rule read commands discover entries in nested `.rule` stores. *(regression test)*
2. All rule transports (cli/mcp/http) resolve identically for the same input. *(parity test)*
3. rule-cli retains no private traversal logic. *(code audit + test)*

## Depends on
- the ticket-domain first run (proven pattern).