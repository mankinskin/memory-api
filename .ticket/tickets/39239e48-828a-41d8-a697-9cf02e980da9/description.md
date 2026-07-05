# Transport-layer workspace-resolution parity (tracker)

## Goal
Make workspace resolution a generic memory-api capability consumed identically by every transport (cli, mcp, http) across every entity domain, and keep transports pure (parse + dispatch only). Fix the reproduced bug where nested child workspaces are invisible to mcp/http and search is non-recursive.

## Principle
- Workspace resolution lives in **memory-api** (one shared resolver); no per-transport or per-domain resolution logic.
- Transports are **pure**: argument parsing + dispatch only.
- Entity search recurses through nested workspaces with one shared traversal/skip policy across `.ticket` / `.spec` / `.rule` / audit stores.

## Foundation
- `ef0ebf38` (in-implementation) — shared memory-api descendant-discovery helper for CLI tools; children `07836f41` (ticket-cli) and `59d96577` (spec-cli + spec-mcp) now in implementation.

## Execution order
1. **Focused first run — TICKET domain** (`27558fde`): establishes shared-resolver adoption for mcp/http + the pure-transport audit pattern (hoist misplaced ticket-cli logic into ticket-api/memory-api).
2. **Follow-up domains** adopt the proven pattern, each `depends_on` the ticket-domain run:
   - SPEC (`5318aedd`)
   - RULE (`1fd0c182`)
   - AUDIT (`632974d1`)

## Done when
All transports across ticket/spec/rule/audit resolve workspaces identically via the shared memory-api resolver; nested-root discovery works recursively over cli/mcp/http; no transport or domain carries duplicate resolution logic; parity tests cover each domain.

## Placement note
memory-api-domain tracker; migration candidate for the memory-api store (see cleanup `7599ed31`).