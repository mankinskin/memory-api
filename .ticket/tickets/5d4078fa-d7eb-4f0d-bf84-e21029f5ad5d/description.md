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

## State note
Cross-store dependency edge G-D -> 8a90a63c (root-store program umbrella) could not be re-added from the memory-api store (entity-not-found); tracked as documentation-only traceability pending cross-workspace edge tooling.