## Goal
Make the feedback system actually usable and self-driving. The feedback-api crate, schema, transports, and redistributed ring edges exist and compile, but the ring has never fired: 3 of 4 edges have no production caller, the MCP transport is registered nowhere, and agents are still routed to the old rule-feedback surface. This ticket wires the three missing layers so signals are collected, discoverable, analyzed, and synthesized into actions.

## Problem (evidence, 2026-07-12)
- `recompute_spec_verified_state` (spec-api/src/verification.rs), `mine_transcript_for_rule_confusion` (session-api/src/transcript_feedback.rs), and `ingest_frontend_feedback` (feedback-api/src/frontend.rs) have ZERO production callers — only re-exports + `#[test]` references.
- Only the missing-rule edge fires, via rule-cli dispatch (`emit_missing_rule_match_signal` -> ticket-api orchestration).
- `feedback-mcp` is registered in no MCP client config; `feedback-cli`/`feedback-http` likewise undiscoverable.
- AGENTS.md Feedback Workflow still points at `rule feedback` / `rule_record_feedback` (old rule-entry feedback), not the feedback-api surface.
- Analysis primitives exist (`low_rated_entities`, `unresolved_note_entities`, `summary_for`) but nothing aggregates them or issues actions.

## Scope
### 1. Discovery wiring
- Rewrite the AGENTS.md Feedback Workflow section to document the feedback-api surface: how to `feedback ingest` a rating/note on any entity URN (rule/spec/ticket), when to run `rule missing-rule`, and how to read `feedback summary`/`inbox`.
- Register `feedback-mcp` in the repo MCP client config so `feedback_ingest`/`feedback_inbox`/`feedback_query`/`feedback_summary`/`feedback_mine` are reachable.
- Add a concise `.agents/instructions/feedback.instructions.md` (or rule entry) encouraging agents to record feedback that improves the codebase.

### 2. Collection wiring
- Call `ingest_frontend_feedback` from the viewer frontends (user + web feedback edge).
- Add a session Stop-hook that runs `mine_transcript_for_rule_confusion` over the just-ended transcript and persists results.

### 3. Analyzer loop (synthesize actions)
- A scheduled/handoff-time analyzer reads `low_rated_entities` + `unresolved_note_entities` and issues actions: file improvement tickets for low-rated rules/specs, and invoke `recompute_spec_verified_state` when a validation execution lands.

## Acceptance criteria
- At least one live end-to-end firing per ring edge (execution->verified, transcript-mining, frontend-ingest, missing-rule), invoked from a registered transport or hook (not a unit test), each with a recorded .test execution.
- `feedback-mcp` reachable from an agent MCP client; AGENTS.md Feedback Workflow references the feedback-api surface, not rule-feedback.
- Analyzer produces at least one synthesized action (ticket or spec recompute) from stored low-rated/unresolved signals, with evidence.
- No dependency cycle introduced; feedback-api remains a pure sink.