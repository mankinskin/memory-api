## Objective

Turn a ticket entity from a single mutable description blob into a structured, partially-frozen, multi-file mini-plan, so that agents can no longer destroy planned content and can read only the parts their role needs.

This ticket is the track root. It owns no implementation; it tracks the eight capability tickets below to completion and treats their validation evidence obligation as discharged by the leaves.

## Requirements

- All eight child tickets satisfy the acceptance criteria of spec `ticket-api/entity/structured-ticket-entities`.
- No child ticket is closed while any spec acceptance criterion it owns is unverified.
- The existing ticket corpus is migrated with zero content loss.
- The leaf tickets, not the root, record the test-api validation executions for their own acceptance criteria.

## Design

Sliced by capability so each child is independently reviewable and verifiable:

1. Parts storage and manifest schema
2. Part write API and mandatory `description_mode`
3. Plan freezing, amendments, and unfreeze-by-transition
4. Typed `[[refs]]` manifest table
5. Projected reads: view profiles and explicit part lists
6. Migration of existing descriptions
7. ticket-viewer surface for parts, refs, and frozen state
8. Agent guidance and rule entries

The root itself stays out of the storage and rendering details; the child tickets own the concrete Rust modules, CLI/MCP/HTTP call sites, and generated instruction surfaces.

## Implementation Steps

1. Close out 5a3d152c first so the manifest can carry stable part ids, `[[parts]]` order, and the reserved `supersedes` field that later tickets depend on.
2. Land 3d952036 next so every description write must name its mode and every caller can target a specific part by stable part id instead of kind/index.
3. Apply 9d69e93d after the write path is serialized, so `[[refs]]` lands on the same manifest model without colliding with the part-schema work.
4. Implement f9e70385 after the manifest and write gate exist, so freezing can mark the planning parts, reject direct writes, and route corrections into amendments.
5. Deliver 4c7b884e once frozen-part semantics exist, so summary/plan/review/full projections can reflect the new manifest layout without ambiguity.
6. Run f65f2b32 only after the freeze contract is in place, so migration can split descriptions into typed parts, re-freeze planned tickets by re-entering `planned`, and preserve history.
7. Finish 89fa0c25 after the read projections are stable, so the viewer can render parts, refs, frozen state, and amendments exactly as the API exposes them.
8. Complete 71e13480 last, so the instruction and rule surfaces document the shipped write, freeze, read, and migration behavior instead of describing work in progress.

## Examples

Failure this track eliminates: an agent recording a review result calls `update_ticket` with a `description` and no mode, silently replacing a 300-line plan with a 5-line review note. Recovery today requires a manual `undo` against `history.ndjson`.

Second failure this track eliminates: agents avoiding overwrite append instead, producing the current 1286-line `description.md` on ticket 61f78a57 where the objective is unfindable.

## Acceptance Criteria

- All eight child tickets are closed.
- Every acceptance criterion in the spec has a linked validation execution in test-api, and the leaf tickets carry those executions.
- A post-migration audit shows no ticket description containing `## Review`, `## Status`, `## Validation`, or `## Handoff` headings that were confidently classifiable.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- interview record: tmp/interview-structured-ticket-entities.md
- source request: transcripts/29-07-2026_ticket-entity-structure/input.clean.md
- code: memory-api/crates/ticket-api/src/model/filesystem.rs
- code: memory-api/crates/ticket-api/src/storage/store.rs

## Authoring convention (dogfood)

Every ticket in this track uses exactly these headings and they are not edited after the ticket reaches `planned`. Review findings, status updates, and validation results go into files under the ticket's `assets/` directory, never into this description. This is the structure the track is building, applied to itself, and it makes the migration tool's heuristics testable against real data.