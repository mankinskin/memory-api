# Umbrella: Dynamic Session Bootstrapping & Context Routing

Tracking ticket for redesigning the agent workflow from always-on static metacognition into just-in-time, session-scoped context curation. Source: `DESIGN_SESSION_BOOTSTRAPPING.md`. Contract: spec `memory-api/session-api/dynamic-session-bootstrapping` (8c880efc).

## Resolved decisions
D1 agent-carried session id + resume · D2 cross-store URN references · D3 client-side rendering · D4 flush per pin + create dir on init · D5 hard-linked auto-pin only · D6 headers-only rendering · D7 remove always-on instructions (bootstrapper-only) · D8 no mode · D9 usage counting + feedback.

## Sequenced roadmap (prerequisites first)
1. **Cross-store references (robust) — BEFORE bootstrapping.**
   - default store prerequisites [82d6ada4 URN resolver] and [6bd67a7a multi-store discovery] are done.
   - memory-api store hard-link source remains textual for now: `b03be2d5` cross-entity edges; `f00291a3` ticket↔spec integration.
2. **Curation core (usage + feedback) — BEFORE bootstrapping.**
   - gate on [c7542933 feedback-api CORE curation surface], not the full feedback-api program.
3. **Design & contract** — afa00b5c (resolved contract; close when review is complete).
4. **session-api runtime model** — 412964a3 (depends on design + URN resolver + curation core).
5. **Cascade context gathering** — d8f76965 (provisional; do not implement before hard-link work lands).
6. **CLI + MCP surfaces** — 6b2dc497 (depends on runtime + cascade + rating surface).
7. **Rule rendering redesign** — b4a8dc5e (depends on CLI/MCP).

## Children
afa00b5c · c7542933 · 412964a3 · d8f76965 · 6b2dc497 · b4a8dc5e (epic closes when all required slices reach done; cascade remains intentionally provisional until hard-link prerequisites are real).

## Key risk — ticket-store boundary
The session-bootstrap tickets live in the **default** `.ticket` store; the hard-link cross-entity-edge prerequisites live in the **memory-api** `.ticket` store. `add_edge` resolves both endpoints within one workspace, so those cross-store prerequisites are still tracked textually until the hard-link work co-locates the relevant entities.