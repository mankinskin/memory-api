# Design: session bootstrap contract & rendering redesign plan

Planning/design ticket for the [session-bootstrap] epic. Produces the specs and the resolved decisions.

## Status: RESOLVED — contract frozen
D1–D9 frozen in parent spec `memory-api/session-api/dynamic-session-bootstrapping` (8c880efc); R1–R5 refinements and Q1–Q5 settlements recorded in that spec's sections. The session_context schema + ADRs are now frozen (see "ADR Freeze" section).

- D1 session identity = agent-carried id; runtime context in the capture session dir.
- D2 cross-store references via URNs `ce://<ws>/<store>/<id>` (hard prerequisite).
- D3 client-side rendering.
- D4 flush per pin; create session dir on init.
- D5 cascade auto-pins hard-linked entities only.
- D6 headers-only rendering; bodies fetched explicitly.
- D7 remove always-on instructions; bootstrapper-only + discoverable/pinned rules.
- D8 no `current_mode`; every session bootstraps (general chat = empty pins).
- D9 usage counting + feedback ratings (gated on feedback-api CORE `c7542933` per Q1).

## Specs authored
- Parent contract: `memory-api/session-api/dynamic-session-bootstrapping` (8c880efc)
- `memory-api/session-api/runtime-session-context` (709f067a) — frozen session_context.json schema (schema_version 1)
- `memory-api/session-api/cascade-context-gathering` (fda5c915)
- `memory-api/session-api/minimal-bootstrapper-selective-loading` (a28a88db)
- `memory-api/curation/entity-usage-and-feedback` (71b81a55)

## Remaining design work — RESOLVED
- Rule-URN shape: FROZEN as ADR-2 — pinned rules reference a single rule entry `ce://<ws>/rule/<entry-id>` (slug alias accepted); `filter` is agent metadata, not identity. See spec ADR Freeze section.
- Always-on `applyTo` glob removal (D7 risk): CONFIRMED acceptable (R3 always-bootstrap). ADR-4.
- Store consolidation vs cross-store edges: RESOLVED — lowest-owning-store placement (Q2) + move-tooling consolidation (R1). ADR-3.

## session_context schema + ADRs
Frozen in spec 8c880efc section "ADR Freeze — session_context schema + rule-URN shape (closes design afa00b5c)" (ADR-1..ADR-4) and child spec 709f067a (schema_version 1).

## Done when — MET
Specs reviewed; remaining design work resolved; ADRs frozen so implementation children are unblocked by their cross-store + usage prerequisites. Implementation children (412964a3, 6b2dc497, b4a8dc5e, c7542933) and cross-store prerequisites (82d6ada4, 6bd67a7a) now build against a frozen contract. Cascade (d8f76965) stays provisional per R2 until hard links land.