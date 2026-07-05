# Spec-domain transport workspace-resolution parity

Follow-up after the ticket-domain first run. Adopt the same shared memory-api resolver + pure-transport pattern for the spec transports.

## Scope
- spec-cli + spec-mcp + spec-http consume the shared memory-api workspace resolver; nested `.spec` discovery is recursive from a parent workspace.
- Coordinate with `59d96577` (spec-cli + spec-mcp root-awareness, in implementation): extend to spec-http, remove the spec refs-only fallback path, and complete the pure-transport audit.
- No per-transport resolution logic remains in any spec transport.

## Acceptance criteria (test-validatable)
1. From a parent workspace, spec get/search/list/refs-validate discover specs in nested `.spec` stores. *(regression test)*
2. All spec transports (cli/mcp/http) resolve identically for the same input. *(parity test)*
3. The refs-only fallback path is removed; resolution comes only from the shared resolver. *(code audit + test)*

## Depends on
- the ticket-domain first run (proven pattern).
- `59d96577` (spec-cli + spec-mcp root-awareness).