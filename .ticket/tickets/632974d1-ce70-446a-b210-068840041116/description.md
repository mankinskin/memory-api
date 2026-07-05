# Audit-domain transport workspace-resolution parity

Follow-up after the ticket-domain first run. Adopt the shared memory-api resolver + pure-transport pattern for the audit transports.

## Scope
- audit-cli + audit-mcp (+ audit-http if present) consume the shared memory-api workspace resolver; nested store discovery is recursive from a parent workspace.
- Audit each transport for private resolution/store-selection logic; hoist into `audit-api` / `memory-api`.
- No per-transport resolution logic remains in any audit transport.

## Acceptance criteria (test-validatable)
1. From a parent workspace, audit read commands resolve nested stores consistently. *(regression test)*
2. All audit transports resolve identically for the same input. *(parity test)*
3. No audit transport carries resolution logic absent from the shared resolver. *(code audit)*

## Depends on
- the ticket-domain first run (proven pattern).