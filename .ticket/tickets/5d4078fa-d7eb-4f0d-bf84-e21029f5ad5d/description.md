## G-D — Feedback ring

Close the open loop so the system improves itself. Extends the existing feedback-api set (b1e9e744 inbox, 9c95c1e4 ingestion, 4f86d3d2 governance, 3a1ec9f8 SLOs) and the audit health loop (bd1c7cc0); does not rebuild them.

## New ring semantics
1. **Execution → spec verified recompute** — a validation execution outcome recomputes the linked spec's `verified` state (ties to G-A guards).
2. **Transcript mining** — review session transcripts to detect good/bad tool usage and rule confusion; surface as feedback.
3. **Missing-rule auto-ticketing** — when a session situation query surfaces no matching rule, file a "missing-rule" ticket to fill the gap.
4. **User + web-frontend feedback** — capture feedback from user interviews and the viewer frontends, not only agent-invoked ratings.
5. **Ticket-entity feedback gap** — add direct feedback/ratings on ticket entities (rule + spec feedback already exist; ticket is the flagged gap).

## Acceptance criteria
- Ring edges are defined: execution→verified, transcript→feedback, no-match→missing-rule-ticket.
- Ticket-entity feedback closes the coverage gap.
- New work attaches to feedback-api b1e9e744 and program umbrella 8a90a63c rather than duplicating.