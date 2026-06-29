# [tools] Close transport surface gaps (or document intentional absences)

## Goal

The transport surfaces are heterogeneous, which forces the e2e matrix (#1) to blanket-block whole domains. Decide and act: build the missing surfaces, or formally document them as out-of-scope so the matrix marks them Blocked honestly.

## Current surface inventory

- CLI (6): ticket, test, spec, session, rule, audit — **missing doc, log**
- HTTP (3): ticket, spec, doc — **missing test, log, rule, session, audit**
- MCP (6): ticket, test, spec, session, rule, audit — **missing doc, log**

## Scope

- A decision record (D8) for each missing `(domain, transport)`: build vs. out-of-scope, with rationale.
- For "build": implement the minimal surface needed for the matrix (e.g. `log-cli get`, `test-http list`).
- For "out-of-scope": a documented reason the matrix can cite in its Blocked cells.

## Acceptance criteria

- [ ] Every missing `(domain, transport)` has an explicit build-or-document decision.
- [ ] Any surfaces chosen for "build" are implemented and consumed by the matrix (#1).
- [ ] The matrix's Blocked reasons cite the documented decisions, with no silent skips.

## Open decisions

- D8: which gaps to build vs. document; priority order; minimal vs. full surface for newly-built ones.

## Relationship / traceability

- Blocks/feeds the transport matrix (#1).
- Touches `memory-api/tools/cli`, `tools/http`, `tools/mcp`.
