<!-- aligned-structure:v2 -->

# Summary

Define a first-class feedback platform contract where `feedback-api` owns persistent, versioned feedback records and the feedback ring is a distributed architectural property across owning domains, not a single module.

## Motivation ("why")

The current feedback subsystem was embedded inside `rule-api`, leaving no explicit crate boundary for ingestion/query/governance and enabling done-state tickets to certify a domain that had no owning crate. This contract establishes `feedback-api` as the canonical store and schema owner so dependents can rely on durable feedback behavior.

## Dependent expectation

If this spec is implemented, dependents can rely on a first-class `feedback-api` crate exposing a single versioned `FeedbackEntry` contract with persisted ingestion, inbox/query access, summary aggregation, and distributed ring persistence semantics where each edge is owned by its domain (`spec-api` execution->verified recompute, `session-api` transcript mining, `ticket-api` missing-rule follow-up, `feedback-api` frontend/user ingest).

## Guards

The verification of this specification contract is gated by:
- `val-feedback-api-store-roundtrip` (ensures ingest/write/read round-trips for `FeedbackEntry`, usage, and rating events)
- `val-feedback-api-ring-persistence` (ensures distributed domain-owned ring edges persist feedback artifacts)
- `val-feedback-api-transport-surface` (ensures CLI, MCP, and HTTP entrypoints expose ingest, inbox/query, mine, and summary paths)

## Positions

- Core schema + store ownership: `implemented` at [./memory-api/crates/feedback-api/src/lib.rs](./memory-api/crates/feedback-api/src/lib.rs)
- Persistent NDJSON store operations: `implemented` at [./memory-api/crates/feedback-api/src/feedback_store.rs](./memory-api/crates/feedback-api/src/feedback_store.rs)
- Rule API dependency inversion (`rule-api -> feedback-api`): `implemented` at [./memory-api/crates/rule-api/src/feedback.rs](./memory-api/crates/rule-api/src/feedback.rs)
- Frontend/user ingestion edge: `implemented` at [./memory-api/crates/feedback-api/src/frontend.rs](./memory-api/crates/feedback-api/src/frontend.rs)
- Execution->verified recompute edge: `implemented` at [./memory-api/crates/spec-api/src/verification.rs](./memory-api/crates/spec-api/src/verification.rs)
- Transcript mining edge: `implemented` at [./memory-api/crates/session-api/src/transcript_feedback.rs](./memory-api/crates/session-api/src/transcript_feedback.rs)
- Missing-rule follow-up orchestration edge: `implemented` at [./memory-api/crates/ticket-api/src/missing_rule.rs](./memory-api/crates/ticket-api/src/missing_rule.rs)
- Rule no-match signal seam: `implemented` at [./memory-api/crates/rule-api/src/no_match.rs](./memory-api/crates/rule-api/src/no_match.rs)
- CLI transport: `implemented` at [./memory-api/tools/cli/feedback-cli/src/main.rs](./memory-api/tools/cli/feedback-cli/src/main.rs)
- MCP transport: `implemented` at [./memory-api/tools/mcp/feedback-mcp/src/server.rs](./memory-api/tools/mcp/feedback-mcp/src/server.rs)
- HTTP transport: `implemented` at [./memory-api/tools/http/feedback-http/src/lib.rs](./memory-api/tools/http/feedback-http/src/lib.rs)

## Governing-rule requirement

This specification is governed and introduced by:
- [shared/instructions/spec-system/spec-system-guidance/spec-authoring-workflow/structure-the-spec/l52](shared/instructions/spec-system/spec-system-guidance/spec-authoring-workflow/structure-the-spec/l52)

## Traceability

- [5d4078fa G-D wiring](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/5d4078fa-d7eb-4f0d-bf84-e21029f5ad5d/ticket.toml)
- [b1e9e744 FB-INBOX](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/b1e9e744-aeac-474a-91d9-07e3a362dc76/ticket.toml)
- [9c95c1e4 FB-INGEST](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/9c95c1e4-3cdb-428e-b9de-800684651226/ticket.toml)
- [c7542933 FB-CURATION](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/c7542933-3052-45c8-99e6-3e09f40cc9b9/ticket.toml)
- [fb6aa078 feedback-api design](C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/fb6aa078-1f8b-4a76-99bd-3a26190b1208/ticket.toml)
