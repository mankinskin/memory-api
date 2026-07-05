# Ticket-domain transport workspace-resolution parity (focused first run)

The **first** parity run. Establishes the shared-resolver adoption + pure-transport audit pattern that the spec/rule/audit domains will reuse.

## Problem (reproduced 2026-06-27)
- ticket-mcp is effectively limited to the `default` workspace; `list_workspaces` hides the nested `memory-api/.ticket` store.
- ticket search from the parent/default workspace does NOT discover tickets that live only in a nested child store (the `7f4aaa05` case); targeting the child root inconsistently aggregates the parent.
- ticket-cli carries resolution / store-selection logic that should live in `ticket-api` / `memory-api`.

## Scope
- ticket-mcp + ticket-http consume the shared memory-api workspace resolver (from `ef0ebf38`): accept a workspace/root selector, support nested-root discovery, and expose descendant workspaces (fix `list_workspaces`).
- Audit ticket-cli for resolution/store-selection logic; hoist it into `ticket-api` / `memory-api` so cli/mcp/http share one path. Transports become pure (parse + dispatch).
- Define the parity test pattern the other domains reuse.

## Acceptance criteria (test-validatable)
1. From the parent (context-engine) workspace, ticket search/get/list discover tickets that live only in the nested `memory-api/.ticket` store. *(regression test reproducing the 7f4aaa05 case)*
2. ticket-mcp exposes and can target nested child workspaces, matching ticket-cli `--index-root` behavior. *(mcp-level test)*
3. All three ticket transports (cli/mcp/http) resolve identically for the same input; ticket-cli duplication removed. *(code audit + parity test)*
4. The shared resolver applies one traversal/skip policy across hidden stores. *(unit test)*

## Depends on
- `ef0ebf38` — shared memory-api descendant-discovery helper (CLI groundwork) is reused here for mcp/http.