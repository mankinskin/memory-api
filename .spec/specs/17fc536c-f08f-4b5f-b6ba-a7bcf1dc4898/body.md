<!-- aligned-structure:v2 -->

# Summary

Define a first-class feedback platform contract where `feedback-api` owns persistent, versioned feedback records and the feedback ring is a distributed architectural property across owning domains, not a single module.

## Motivation ("why")

The current feedback subsystem was embedded inside `rule-api`, leaving no explicit crate boundary for ingestion/query/governance and enabling done-state tickets to certify a domain that had no owning crate. This contract establishes `feedback-api` as the canonical store and schema owner so dependents can rely on durable feedback behavior.

## Dependent expectation

If this spec is implemented, dependents can rely on a first-class `feedback-api` crate exposing a single versioned `FeedbackEntry` contract with persisted ingestion, inbox/query access, summary aggregation, and distributed ring persistence semantics where each edge is owned by its domain (`spec-api` execution->verified recompute, `session-api` transcript mining, `ticket-api` missing-rule follow-up, `feedback-api` frontend/user ingest). Dependents mining structured session signals can rely on: (a) `FeedbackProvenance` carrying session-turn/tool-call backtrace refs (schema v2), (b) explicit-ingestion and failed-tool-call signals being mined only from `CopilotHookEvent`s (never assumed from transcript turns, which have no `role: tool` entries in real data), (c) a grounded, never-guessing failed-tool-call-to-entity mapping policy, (d) a deterministic deduped BFS entity-discovery queue, and (e) idempotent, backtraceable follow-up ticket synthesis gated to successful non-helpful ingestion signals. Dependents relying on `test-mcp`/`test-api` evidence for this contract's guards cannot yet fully rely on evidence durability or discoverability — see the two open evidence-integrity tickets in Traceability.

## Guards

The verification of this specification contract is gated by:
- `val-feedback-api-store-roundtrip` (ensures ingest/write/read round-trips for `FeedbackEntry`, usage, and rating events)
- `val-feedback-api-ring-persistence` (ensures distributed domain-owned ring edges persist feedback artifacts)
- `val-feedback-api-transport-surface` (ensures CLI, MCP, and HTTP entrypoints expose ingest, inbox/query, mine, and summary paths)
- `vt-feedback-provenance-backtrace` (`FeedbackProvenance` session/turn/tool-call backtrace refs round-trip and pre-v2 compatibility)
- `vt-feedback-explicit-ingestion-mining` (explicit feedback-ingestion tool calls mined from structured session events)
- `vt-feedback-failed-tool-call-mapping` (failed-tool-call to entity mapping policy; grounded, never-guessing)
- `vt-feedback-bfs-entity-queue` (deterministic deduped BFS entity-discovery queue)
- `vt-feedback-followup-synthesis` (backtraceable, idempotent follow-up ticket synthesis)

## Positions

- Core schema + store ownership: `implemented` at [./memory-api/crates/feedback-api/src/lib.rs](./memory-api/crates/feedback-api/src/lib.rs) — schema v2 adds `turn_sequence`/`tool_call_id` backtrace refs and `FeedbackProvenance::from_session_turn`.
- Persistent NDJSON store operations: `implemented` at [./memory-api/crates/feedback-api/src/feedback_store.rs](./memory-api/crates/feedback-api/src/feedback_store.rs)
- Rule API dependency inversion (`rule-api -> feedback-api`): `implemented` at [./memory-api/crates/rule-api/src/feedback.rs](./memory-api/crates/rule-api/src/feedback.rs)
- Frontend/user ingestion edge: `implemented` at [./memory-api/crates/feedback-api/src/frontend.rs](./memory-api/crates/feedback-api/src/frontend.rs)
- Execution->verified recompute edge: `implemented` at [./memory-api/crates/spec-api/src/verification.rs](./memory-api/crates/spec-api/src/verification.rs)
- Transcript mining edge: `implemented` at [./memory-api/crates/session-api/src/transcript_feedback.rs](./memory-api/crates/session-api/src/transcript_feedback.rs) — event-based `mine_explicit_ingestion_signals`, `mine_failed_tool_call_signals`, grounded `map_failed_tool_call_to_entity`/`FailedToolCallMapping`/`UnmappedReason`, and `EntityDiscoveryQueue` BFS mining.
- Follow-up ticket synthesis edge: `implemented` at [./memory-api/crates/session-api/src/follow_up.rs](./memory-api/crates/session-api/src/follow_up.rs) — re-enabled behind a backtraceable description format and idempotent UUIDv5 ticket keying; wired into the Stop hook at [./memory-api/crates/session-api/src/bin/copilot-capture-hook.rs](./memory-api/crates/session-api/src/bin/copilot-capture-hook.rs).
- Missing-rule follow-up orchestration edge: `implemented` at [./memory-api/crates/ticket-api/src/missing_rule.rs](./memory-api/crates/ticket-api/src/missing_rule.rs)
- Rule no-match signal seam: `implemented` at [./memory-api/crates/rule-api/src/no_match.rs](./memory-api/crates/rule-api/src/no_match.rs)
- CLI transport: `implemented` at [./memory-api/tools/cli/feedback-cli/src/main.rs](./memory-api/tools/cli/feedback-cli/src/main.rs)
- MCP transport: `implemented` at [./memory-api/tools/mcp/feedback-mcp/src/server.rs](./memory-api/tools/mcp/feedback-mcp/src/server.rs)
- HTTP transport: `implemented` at [./memory-api/tools/http/feedback-http/src/lib.rs](./memory-api/tools/http/feedback-http/src/lib.rs)
- Ring activation wiring (discovery/collection/analyzer loop end-to-end): `partial` — code-level mining/synthesis edges above are implemented and unit/e2e tested; live end-to-end firing against a real production session is not yet verified (tracked by ACT below).
- Evidence durability for the guards above: `partial` — validation-spec and execution records exist for all 5 `vt-feedback-*` guards at [./memory-api/.test/default](./memory-api/.test/default), but the underlying `test-mcp`/`test-api` read/write path has two open integrity gaps (store routing split, destructive global pruning) tracked below; do not treat guard evidence as durable until both are resolved.

## Governing-rule requirement

This specification is governed and introduced by:
- [shared/instructions/spec-system/spec-system-guidance/spec-authoring-workflow/structure-the-spec/l52](shared/instructions/spec-system/spec-system-guidance/spec-authoring-workflow/structure-the-spec/l52)

## Traceability

- [5d4078fa G-D wiring](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/5d4078fa-d7eb-4f0d-bf84-e21029f5ad5d/ticket.toml)
- [b1e9e744 FB-INBOX](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/b1e9e744-aeac-474a-91d9-07e3a362dc76/ticket.toml)
- [9c95c1e4 FB-INGEST](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/9c95c1e4-3cdb-428e-b9de-800684651226/ticket.toml)
- [c7542933 FB-CURATION](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/c7542933-3052-45c8-99e6-3e09f40cc9b9/ticket.toml)
- [fb6aa078 feedback-api design](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/fb6aa078-1f8b-4a76-99bd-3a26190b1208/ticket.toml)
- [6b0002bf ring activation (ACT)](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/6b0002bf-c15b-4cc7-ac38-fbf66a07d1bc/ticket.toml) — parent epic wiring discovery/collection/analyzer loop end-to-end; in-review, blocked on review of the 6 tickets below.
- [a7601cb7 PROV](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/a7601cb7-6c92-4891-aa55-07ab46125bb8/ticket.toml) — `FeedbackProvenance` session/turn/tool-call backtrace refs; in-review.
- [b4954d6c INGEST](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/b4954d6c-0451-49ed-8939-11f6568558f5/ticket.toml) — mine explicit feedback-ingestion tool calls from structured session events; in-review.
- [16e112a7 MAP](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/16e112a7-5ab0-45a5-87c8-7d89d07ffd16/ticket.toml) — failed-tool-call to entity mapping/recording policy; in-review.
- [3fa60398 BFS](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/3fa60398-3154-46ee-aca5-8d87541bac1e/ticket.toml) — deterministic BFS entity-discovery queue; in-review.
- [3d4c4739 SYNTH](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/3d4c4739-3138-40be-947d-556e5f7de53a/ticket.toml) — backtraceable/idempotent follow-up synthesis re-enablement; in-review.
- [905d05ae test-mcp evidence routing fix](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/905d05ae-b367-44f8-9988-a671702d8a32/ticket.toml) — fixed and in-review: `test-mcp` read tools now aggregate every discoverable `.test` store (or pin to one via explicit `workspace`) instead of only reading the server's fixed root.
- [1e8f6866 test-api destructive pruning bug](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/1e8f6866-9dda-4b2c-9f41-27ac83ee61d5/ticket.toml) — critical, open: `prune_execution_runs` globally keeps only the 2 newest distinct `run_id`s across the *entire* `.test` store and silently deletes every other execution, regardless of ticket/spec ownership. Reproduced live this session: recording PROV/INGEST/MAP evidence cascaded and deleted BFS/SYNTH evidence and, separately, 6 git-tracked DESIGN (fb6aa078) executions; both incidents were recovered (git restore + re-recording under one shared `run_id`), but the store still holds 3 distinct `run_id`s today, one over the keep-2 threshold, so the next unrelated write remains at risk of deleting further evidence until this is fixed.
