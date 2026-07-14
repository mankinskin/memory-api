# Epic: Dynamic Session Bootstrapping and Durable Workflow Context

Redesign agent startup and continuation around a durable logical session workspace rather than always-on static context or transcript replay.

## Product contract

A workspace session carries:

- a durable `workspace_session_id` spanning handoffs;
- distinct linked capture `run_id` values per agent execution;
- pinned ticket, spec, and rule URNs;
- a mutable workflow graph containing ticket-backed and session-only nodes;
- live ticket-state resolution;
- terminal and Mermaid roadmap rendering;
- structured handoff records and exact resume commands;
- explicit graph-gated finish.

Feedback usage/rating emission is an optional integration and cannot block context or workflow persistence.

## Sequenced roadmap

1. **Design and addressing foundations — done.** Frozen bootstrap decisions `afa00b5c`; URN resolver `82d6ada4`; multi-store discovery `6bd67a7a`.
2. **Runtime pinned context — next implementation leaf.** `412964a3` adds durable workspace identity, run lineage, pin/unpin/read/view, and optional feedback sink behavior.
3. **Durable workflow core.** `70cd7056` persists and mutates ticket-backed plus session-only workflow nodes and edges.
4. **Parallel workflow consumers.** `cc4b0289` renders terminal/Mermaid graphs; `0647a212` persists handoffs, resumes new runs, and enforces finish gates.
5. **CLI/MCP surfaces.** `6b2dc497` exposes context, workflow, rendering, handoff/resume, and finish commands.
6. **Generated handoff guidance.** `9577b114` requires every `/handoff` to carry the durable ID and exact resume flow.
7. **Cascade auto-discovery.** `d8f76965` remains optional follow-up work blocked on structured hard links `b03be2d5` and `f00291a3`; manual pin/workflow commands do not wait for it.
8. **Minimal selective loading.** `b4a8dc5e` follows the usable CLI/MCP context surface.

## Planning and review cleanup

- Parent contract: `8c880efc-7083-4e1d-bf06-96b8254be913`.
- Durable workflow contract: `c677182e-90da-4ac3-8b94-9e2e97c825cf`.
- Runtime and handoff child specs must use aligned-structure v2 and accurate implementation positions.
- Core feedback `c7542933` has implementation and focused validation recorded and should complete independent review; ingestion/full feedback work is outside this epic's critical path.

## Done when

The session API can initialize or resume a durable workspace, manage pinned entities and an evolving roadmap, render the roadmap in terminal or Mermaid form, persist a structured handoff, resume under a new linked run, and reject finish until required work and validation gates are satisfied. Generated `/handoff` guidance always carries the durable workspace ID and exact resume command.