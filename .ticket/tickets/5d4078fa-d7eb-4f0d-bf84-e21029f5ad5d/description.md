## G-D — Feedback ring (redistribution, not centralization)

Close the open loop so the system improves itself. The ring is an **emergent distributed loop**, not a module: every domain writes feedback into the feedback-api hub and reacts to outcomes. Extends the feedback-api set (b1e9e744 inbox, 9c95c1e4 ingestion, c7542933 curation core, 4f86d3d2 governance, 3a1ec9f8 SLOs) and the audit health loop (bd1c7cc0); does not rebuild them. Design/boundary work tracked in fb6aa078.

## Decision (grounded 2026-07-12 review)
The first implementation dumped all ring edges into `rule-api::ring`, forcing a rules crate to depend on spec-api + test-api + session-api + ticket-api. This is wrong. `rule-api::ring` must be **deleted as a module** and its edges redistributed to owning domains (see fb6aa078 edge-ownership table). feedback-api is the hub, not rule-api.

## Ring edges (redistributed ownership)
1. **execution → spec verified recompute** — owned by spec-api (spec×test seam); emits System feedback via feedback-api.
2. **transcript mining** — owned by session-api / transcript analyzer; emits TranscriptMined feedback; drop string heuristics.
3. **missing-rule auto-ticketing** — rule-api emits only the no-match signal; ticket+feedback orchestration is over ticket-api/feedback-api.
4. **user + web-frontend feedback** — pure feedback-api ingest path.
5. **ticket-entity feedback gap** — direct feedback/ratings on ticket entities (rule + spec already covered).

## Acceptance criteria
- No `ring` module in rule-api; edges live in their owning domains; rule-api keeps only the rule-match signal.
- rule-api Cargo.toml no longer depends on spec-api/test-api/session-api.
- All redistributed edges persist through feedback-api; per-domain edge tests pass.
- Ticket-entity feedback closes the coverage gap.
- New work attaches to feedback-api b1e9e744 and program umbrella 8a90a63c (root store) rather than duplicating.

## Activation gate — 'Close' vs 'Redistribute' (added 2026-07-12 verification)
Redistribution + unit tests satisfy **'redistribute'**, NOT **'close'**. Verified 2026-07-12: 3 of 4 ring edges (recompute_spec_verified_state, mine_transcript_for_rule_confusion, ingest_frontend_feedback) have ZERO production callers — only re-exports + `#[test]` references; only missing-rule fires (rule-cli dispatch). A redistributed loop of dead code is still dead code.
- **'Close' requires ≥1 live end-to-end firing per ring edge** (execution→verified, transcript-mining, frontend-ingest, missing-rule) invoked from a **registered transport or hook — not a unit test** — each with a recorded .test execution.
- This ticket now **depends_on 6b0002bf** ([feedback-api][activation] discovery/collection/analyzer wiring). G-D cannot reach `done` until activation lands a live firing per edge.
- If activation is deferred, **rename this ticket to "Redistribute ring edges"** and let 6b0002bf carry closure.

## State note
Cross-store dependency edge G-D -> 8a90a63c (root-store program umbrella): add it from the ROOT workspace (`--workspace default`), since 8a90a63c-0a07-439f-90e8-9124212b2dc8 is not resolvable from the memory-api store (entity-not-found). Prefer this over documentation-only traceability.