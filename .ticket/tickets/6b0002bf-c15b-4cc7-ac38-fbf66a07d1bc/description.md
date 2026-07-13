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

## Session progress — 2026-07-13 (robust-model rework)
Discovery/collection/analyzer wiring landed earlier in the track; this session hardened the model after review found the miner was a false-positive factory.

Landed + validated this session:
- Removed free-text transcript mining (tokenizer + "confusion markers" + `overlap < 2`) that had spam-created **522** false-positive `[feedback-loop]` tickets (now bulk-deleted). Replaced with structured-metadata-only detection in `crates/session-api/src/transcript_feedback.rs`: `mine_structured_feedback_signals` detects failed tool calls via `event_meta.tool_success == Some(false)` and returns backtraceable `StructuredFeedbackSignal` (tool_call_id/event_id). No `SessionTurn.content` inspection.
- Stop-hook (`crates/session-api/src/bin/copilot-capture-hook.rs`) no longer auto-creates tickets — it logs a structured signal summary. Auto-synthesis is PAUSED pending a backtraceable/verifiable ticket format.
- Typed spec verification: `recompute_spec_verified_state` returns `SpecVerificationOutcome` (`NoGuards` / `Pending{missing_guards}` / `Failed{failed_guards}` / `Verified`) instead of the ambiguous collapsed `Ok(false)`. `test-cli` now emits structured `spec_verification` reports instead of swallowing errors into a `Vec<String>`.
- Fixed 5 pre-existing `test-cli` fixture failures caused by the committed interoperability provenance contract (commit 745d2ef) — added `domain`/`operation`/`run_id`/links.
- Kept `.session` transcripts tracked in git per decision (reverted an interim gitignore change).

Validation: session-api 13/13, spec-api verification 3/3 (2 new typed-outcome tests), test-cli 8/8. Evidence: test-api execution `exec-vt-feedback-ring-robust-model-20260713` (passed), spec `vt-feedback-ring-robust-model`.

## Session progress — 2026-07-13 (ring reactivation, all 6 dependencies closed)
All six dependency tickets are now closed, each with recorded validation evidence:
- `fb6aa078` — stale design ticket verified (ring.rs deleted, rule-api Cargo.toml drops spec-api/test-api/session-api, feedback-api+rule-api tests pass) and closed.
- `a7601cb7` (PROV) — `FeedbackProvenance` extended with `turn_sequence`/`tool_call_id`; schema bumped to v2, backward-compatible. `vt-feedback-provenance-backtrace`.
- `b4954d6c` (INGEST) — **Critical grounding finding**: real `.session/sessions/*` transcripts have zero `role: tool` `SessionTurn`s (61 sessions, 2670 turns checked) — tool call/result data lives only in the separate `CopilotHookEvent` list. Added `mine_explicit_ingestion_signals` (event-based) + `recover_feedback_entry_from_signal` (only recovers failed live calls, never duplicates a successful one). `vt-feedback-explicit-ingestion-mining`.
- `16e112a7` (MAP) — grounded failed-tool-call distribution (115/5031 failures across 61 sessions); typed `FailedToolCallMapping`/`UnmappedReason` policy; added event-based `mine_failed_tool_call_signals` (the turn-based miner never fires on real data either). `vt-feedback-failed-tool-call-mapping`.
- `3fa60398` (BFS) — deterministic, deduplicated `EntityDiscoveryQueue` + `discover_entities_from_signals`. `vt-feedback-bfs-entity-queue`.
- `3d4c4739` (SYNTH) — re-enabled follow-up ticket synthesis in the stop-hook, gated on successful `ExplicitIngestion` signals with a `not-helpful`/`mixed` rating only (documented rationale for excluding bare failed-tool-call signals); idempotent via a deterministic UUIDv5 ticket id (`follow_up_ticket_id`) so re-running a session never duplicates. `vt-feedback-followup-synthesis`.

All new code lives in `crates/session-api/src/transcript_feedback.rs` and the new `crates/session-api/src/follow_up.rs`, wired into `copilot-capture-hook.rs`. Full workspace (`cargo build --workspace`) and `cargo test -p feedback-api -p session-api` are clean.

**Remaining for this ticket's own close (not yet done):** a genuine live end-to-end firing of the re-enabled synthesis path against a real captured session (not just unit/integration tests with synthetic events) — i.e. running the actual stop-hook script end-to-end and confirming exactly one ticket is synthesized and zero on re-run, plus reconfirming the other three acceptance criteria (feedback-mcp MCP-client registration, AGENTS.md Feedback Workflow wording, analyzer live firing) are still current. Moving to `in-review` for that follow-up verification pass.