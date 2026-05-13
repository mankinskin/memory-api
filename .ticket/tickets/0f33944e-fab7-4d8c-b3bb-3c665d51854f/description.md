## Problem

`spec-api`'s schema declares three edge kinds (`parent_of`, `linked`,
`depends_on`) but `SpecStore` exposes no API to create or query them.
Verified by source grep on `crates/spec-api/src/` — there is no edge
handling code at all. The current `parent` field on `SpecManifest` is a
plain string, and `spec tree` walks those strings rather than real edges.

This blocks cross-component navigation (e.g. linking
`spec-api/store` ↔ `memory-api/storage/entity-store`) needed by ticket
[13a57a83](.ticket/tickets/13a57a83-df99-4031-87e2-844772758ebb/) (spec-system
spec authoring).

## Goal

Add an edge layer to `spec-api` mirroring `ticket-api`'s edge surface, so
specs can be linked across crates and the hierarchy is queryable as a graph.

## Scope

- Expose edge create/list/remove APIs on `SpecStore`
  (`add_edge`, `list_edges`, `remove_edge`, `subgraph`).
- Surface those APIs through:
  - `spec-cli`: `spec link`, `spec edges`, `spec subgraph` subcommands.
  - `spec-mcp`: `spec_link`, `spec_edges` tools.
  - `spec-http`: `POST /api/specs/{id}/edges`, `GET /api/specs/{id}/edges`,
    `DELETE /api/specs/{id}/edges/{edge_id}`.
- Update `spec tree` to optionally walk `parent_of` edges instead of the
  manifest `parent` string (with a back-compat fallback).
- Add edge schema validation: `parent_of` is acyclic; `depends_on` is acyclic.
- Add a migration path: `spec migrate-parent-edges` that converts existing
  `parent` string fields into real `parent_of` edges.

## Non-Goals

- Removing the `parent` manifest field. Keep it for back-compat; new code
  prefers edges.
- Edge weights / typed payloads beyond what `memory-api` provides.

## Acceptance Criteria

- [ ] `SpecStore::add_edge(from, to, kind)` creates a row in the edge index.
- [ ] `SpecStore::list_edges(id, direction)` returns connected specs.
- [ ] `parent_of` and `depends_on` edges reject cycles.
- [ ] CLI `spec link --from <a> --to <b> --kind linked` works end-to-end.
- [ ] MCP `spec_link` tool exposed and tested.
- [ ] HTTP edge routes pass integration tests.
- [ ] `spec migrate-parent-edges --dry-run` reports affected specs;
      without `--dry-run` writes edges and verifies `spec tree` matches before/after.
- [ ] Schema-driven cycle detection has unit tests.

## Related

- Blocks part of [13a57a83](.ticket/tickets/13a57a83-df99-4031-87e2-844772758ebb/).
- Pattern reference: `crates/ticket-api/src/storage/` edge handling.
