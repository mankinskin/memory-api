# Umbrella: Dynamic Session Bootstrapping & Context Routing

Tracking ticket for redesigning the agent workflow from always-on static metacognition into just-in-time, session-scoped context curation. Source: `DESIGN_SESSION_BOOTSTRAPPING.md`. Contract: spec `memory-api/session-api/dynamic-session-bootstrapping` (8c880efc).

## Resolved decisions
D1 agent-carried session id + resume · D2 cross-store URN references · D3 client-side rendering · D4 flush per pin + create dir on init · D5 hard-linked auto-pin only · D6 headers-only rendering · D7 remove always-on instructions (bootstrapper-only) · D8 no mode · D9 usage counting + feedback.

## Sequenced roadmap (prerequisites first)
1. **Cross-store references (robust) — BEFORE bootstrapping.**
   - default store prerequisites [82d6ada4 URN resolver] and [6bd67a7a multi-store discovery] are done.
   - memory-api hard-link prerequisites `b03be2d5` cross-entity edges and `f00291a3` ticket↔spec integration are now the concrete remaining cascade prerequisites and should be tracked explicitly from the cascade ticket.
2. **Full feedback-api program — BEFORE bootstrapping.**
   - foundational curation slices [c7542933 feedback-api CORE curation surface] and [9c95c1e4 event ingestion, metadata normalization, and retention policy] are done.
   - runtime bootstrapping still waits on the broader [b1e9e744 feedback inbox, metadata indexing, and deep search] program and its remaining child slices.
3. **Design & contract** — afa00b5c (resolved contract; close when review is complete).
4. **session-api runtime model** — 412964a3 (depends on design + URN resolver + the full feedback-api program).
5. **Cascade context gathering** — d8f76965 (ready for refinement now; do not implement before the hard-link tickets are done and the rule-link shape is finalized).
6. **CLI + MCP surfaces** — 6b2dc497 (depends on runtime + cascade + rating surface).
7. **Rule rendering redesign** — b4a8dc5e (depends on CLI/MCP).

## Children
afa00b5c · b1e9e744 · c7542933 · 412964a3 · d8f76965 · 6b2dc497 · b4a8dc5e (epic closes when all required slices reach done; cascade remains blocked on hard-link completion even after refinement begins).

## Key risk — hard-link completeness
The ticket-store boundary is no longer the main issue here. The remaining risk is that cascade still depends on structured hard links that are not fully delivered yet: `b03be2d5` and `f00291a3` must land, and the rule-entry link shape still needs to be finalized so cascade follows concrete edges instead of inferred text references.