## Problem

Session workflow tools force agents through restrictive, single-item, closed-schema calls that produce avoidable failure cascades. In session `aedf210d-134c-4a8d-ab7c-2060f82f95d4` (turns 60→62→63), persisting 4 review criteria as workflow nodes cost 3 rounds / 12 calls, 8 of them failed:

1. `kind:"review-criterion"` rejected — `kind` is a closed enum (ticket/validation/spec/task); the error listed allowed values but named no alternative pattern.
2. Retried `kind:"validation"` while keeping `ticket_urn` — rejected because "only ticket workflow nodes may set ticket_urn"; the agent wanted to *anchor* a validation node to a ticket and the schema forbade it without suggesting how.
3. Retried again dropping `ticket_urn` — finally succeeded.

## Goal

Let agents build rich, densely-linked, dynamic workflow graphs with minimal friction, and make every unavoidable rejection instantly self-correcting.

## Reviewer-confirmed direction (session aedf210d review)

- Every workflow rejection error must embed a ready-to-use alternative pattern, not just list allowed values.
- Add batch `add_nodes` / `add_edges` operations so many-linked graphs are cheap to build.
- Surface the existing free-text `category` escape hatch at point of use in the `add_node` schema/description.
- Relax URN-gating so any node kind can carry a non-gating anchor URN to a ticket/spec (without changing finish-gating semantics, which stay tied to the `ticket`/`spec`/`validation` kinds).
- Add a new path-scoped guidance file `.agents/instructions/session-workflow.instructions.md` documenting the node-kind model, the `category` escape hatch, URN-anchoring rules, batch usage, and the canonical "review criteria as nodes" pattern.

## Scope

Child tickets:
- Self-correcting workflow rejection errors (session-mcp parse_* + api validation messages).
- Batch node/edge creation tools.
- Surface `category` + relax URN-anchoring in the add_node schema/model.
- Session-workflow authoring instruction file.

## Out of scope

`board_release_lease` returning `ok` while leaving leases active (turns 9/10/20/54) is a ticket-board defect, not a session-workflow issue; track separately if desired.

## Source references

- Node-kind enum + `category` field: memory-api/crates/session-api/src/model/workflow.rs
- URN-gating validation: memory-api/crates/session-api/src/store/config/runtime_workflow.rs
- MCP parse errors + schema: memory-api/tools/mcp/session-mcp/src/server.rs

## Completion

Implemented all four children in semantic order T1 → T3 → T2 → T4.
Rejections now include copy-ready alternatives; `category` and non-gating
`anchor_urn` are exposed and persisted; atomic indexed node/edge batches are
available in session-api, MCP, and CLI; and canonical path-scoped guidance is
hand-owned under `.agents/instructions/` and discoverable through session
bootstrap.

Validation execution `exec-session-workflow-flexibility-20260725` passed
`cargo test -p session-api -p session-mcp -p session-cli`: 146 tests across 12
suites. Existing finish-gating behavior remained green. The guidance
frontmatter has no diagnostics and bootstrap discovery is linked. All four
child tickets are in review. The board release defect remains explicitly out
of scope.