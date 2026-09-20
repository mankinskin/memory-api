# Context-enrichment workflow for review tickets: session-ticket relation query

## Objective

39 tickets sit in `in-review` today, on unrelated topics, with no reliable way to reconstruct
"what was actually done and by which sessions" before closing each one. This spec locks the
design for the first step of a reusable context-enrichment workflow: querying the Session API
for sessions related to a given ticket, so that step can seed context reconstruction, review,
and eventual closure of in-review tickets.

Source interview: `transcripts/04-08-2026_context-enrichment-workflow/input.clean.md`.

## Background / current state

- `SessionQuery` (`memory-api/crates/session-api/src/store.rs#L92-L104`) filters only on
  `session_id_prefix`, `conversation_id`, `agent_id`, `text`, `limit`. There is no ticket filter.
- `session_matches_query` (`memory-api/crates/session-api/src/store/helpers/storage.rs#L438-L480`)
  does not consult `SessionMetadata.ticket_id` or `SessionLinks.ticket_ids` at all.
- The data model already carries the relation:
  - `SessionMetadata.ticket_id: Option<String>` (`memory-api/crates/session-api/src/model.rs#L292-L306`)
    — the ticket a session formally checked in against.
  - `SessionLinks.ticket_ids: Vec<String>` (`memory-api/crates/session-api/src/model/links.rs#L7-L15`)
    — tickets explicitly linked to a session.
- This is a query-surface gap, not a data-model gap.
- No existing ticket or spec covered "context enrichment" or "review closure" before this spec.
- `.agents/agents/review.agent.md` explicitly drops the `session-mcp` tool grant and so cannot
  reconstruct context from session history today.
- `.agents/agents/iteration.agent.md` has `session-mcp` but drives the current session's own
  workflow, not arbitrary-ticket history reconstruction.
- Only `.ticket`, `.spec`, `.session`, `.test`, `.feedback`, `.benchmark` exist as dot-directory
  stores today. There is no `.rule`, `.doc`, or `.log` store, so this spec does not assume they
  exist.

## Locked decisions

1. **Session scope for this workflow slice**: query capability + one dogfood run + a new agent
   template (all three; see linked tickets).
2. **Relation-strength semantics**: three selectable tiers, caller picks a minimum tier:
   - `strict` — `SessionMetadata.ticket_id == ticket_id` only.
   - `linked` — `strict` OR ticket id present in `SessionLinks.ticket_ids`.
   - `mentioned` — `linked` OR ticket id present in the session's handoff package
     `target_tickets`.
   - Bare substring scanning of `transcript.json` is explicitly excluded at every tier — too
     noisy.
3. **API shape**: a separate dedicated entry point, `sessions_for_ticket(ticket_id, strength)`,
   not a new field on `SessionQuery`. Keeps the existing query struct unchanged and lets the
   result type carry richer per-session relation metadata.
4. **Result payload**: each row carries session id, agent id, start/end timestamps, git branch,
   worktree path, and which relation-strength signal matched. The full handoff package body is
   not inlined — callers fetch it separately if needed.
5. **Inverse direction** (Ticket API querying "which sessions worked on me"): explicitly out of
   scope for this spec's tickets, deferred to a follow-up ticket
   (`.ticket/tickets/1ff57502-ad4e-4c40-a852-18752c18f44c/ticket.toml`).
6. **Agent template**: a new, dedicated `.agents/agents/context-enrichment.agent.md`, not an
   extension of `review.agent.md`. Keeps "guide a human reviewer" and "autonomously reconstruct
   and close" as separate contracts.
7. **Dogfood target**: ticket `06cfe998` ([token-efficiency] Introduce peek-api with peek-cli and
   peek-mcp transport layers), currently `in-review`.
8. **Closure authority**: autonomous. When the workflow concludes a ticket's acceptance criteria
   are met (with recorded evidence), it may transition the ticket to `done`/`accepted` without a
   separate human-confirmation step.

## Non-goals

- The Ticket API inverse query is not implemented by this spec's tickets (see decision 5).
- Querying `.rule`, `.doc`, or `.log` stores is out of scope — those stores do not exist on disk
  today.
- Bare transcript-text matching as a relation signal is explicitly rejected, not deferred.

## Invariant: the three tiers each read a different stored field

The `strict ⊆ linked ⊆ mentioned` cumulative relationship (decision 2) is not just a widening
predicate — each tier reads a **different** stored field, populated by a **different**
mechanism. This was implicit in the original locked decisions and cost real rework in the
follow-up session that added linkage capture and backfill; it is recorded here explicitly so it
is not re-derived:

| tier | reads | populated by |
|---|---|---|
| `strict` | `SessionMetadata.ticket_id` | `check_in_worktree`, and (as of this spec's follow-up) capture-time inference |
| `linked` | `SessionLinks.ticket_ids` | backfill from handoff `target_tickets` |
| `mentioned` | handoff package `target_tickets` | already present on disk at capture time |

## Follow-up decisions: linkage capture and historical backfill

Ticket `bba9b313` shipped `sessions_for_ticket` correctly, but a dogfood run against the real
`.session` store (ticket `2b75bac2`) found the query effectively inert: almost no session had
`SessionMetadata.ticket_id` populated. The following decisions extend this spec to cover capture
and backfill, without changing the tier semantics above.

1. **Backfill signal precedence ("split by signal strength")**. Evaluated per session, first
   match wins among the strict-tier signals:
   1. `branch` matching `agent/<short-id>-<slug>` (short-id = 8 hex chars) → written to
      `SessionMetadata.ticket_id` (**strict** tier).
   2. `worktree_path` encoding the same `<short-id>-<slug>` pattern under `.worktrees/` →
      **strict** tier.
   3. handoff package `target_tickets` (a list, so multi-valued) → written to
      `SessionLinks.ticket_ids` (**linked** tier).

   Rationale: `branch` and `worktree_path` are single-valued and high-confidence, so they earn
   the strict tier; handoff targets are multi-valued and weaker evidence, so they earn the
   lower `linked` tier rather than `strict`.

2. **No provenance marker — and the risk this carries.** The user explicitly declined a marker
   distinguishing inferred linkage from linkage declared at check-in time. Consequence: an
   incorrectly inferred link is **permanently indistinguishable** from ground truth once
   written — there is no field to later say "this was a guess." This is judged acceptable only
   because of two mitigations, both of which are normative invariants, not implementation
   details:
   - The tier split in decision 1 above already reflects each signal's confidence, so an
     inference lands at the tier its evidence actually supports.
   - **An unresolvable short id is never written.** Linkage is only persisted after the parsed
     short id resolves to a real ticket in the ticket store. A branch or worktree path that
     merely looks like the pattern but does not resolve produces no write, at any tier.
   - An explicit `check_in_worktree` always outranks inference, and inference never overwrites
     an existing worktree assignment.

3. **Correction: forward capture was never broken.** Earlier analysis in this workflow assumed
   `check_in_worktree` failed to persist linkage. That assumption was wrong: `check_in_worktree`
   and the `strict`-tier lookup have always agreed on `SessionMetadata.ticket_id`, proven by a
   test predating the follow-up session. This correction is recorded so the false hypothesis is
   not re-derived by a later reader.

4. **Resolution: capture-time inference.** The Copilot capture hook
   (`memory-api/crates/session-api/src/bin/copilot-capture-hook.rs`) now records `branch` and
   `worktree_path` at capture time and populates `ticket_id` when the branch matches
  `agent/<short-id>-<slug>` and resolves to a live ticket. **When no ticket link resolves, capture MUST persist the session with zero ticket links and exit successfully; transcript parse, schema, and durable-write failures remain errors.** `main`, detached HEAD, and non-git directories all yield no
  `ticket_id` quietly. This is a normative durability guarantee about session capture, not an
   implementation choice, because capture is on the critical path for every agent turn.

5. **Reaffirmed prohibition.** Scanning session transcript text for linkage remains forbidden at
   every tier, per the original non-goals above. Every mechanism in this follow-up derives
   linkage from structured fields only (`branch`, `worktree_path`, handoff `target_tickets`).
   This constrained every decision in the follow-up session and is restated here because it is
   easy to erode incrementally under pressure to "just check the transcript."

## Measured evidence: backfill dry-run against the real store

A dry-run of the backfill (decision 1 above) against the real `.session` store, 231 sessions
total:

| signal | yield |
|---|---|
| `branch` | 0 / 231 |
| `worktree_path` | 0 / 231 |
| handoff `target_tickets` | 37 associations across 4 sessions |
| corrupt entries skipped | 2 |
| projected coverage | 0.0% → ~1.7% |

Root cause: no session in the store had a worktree assignment at all, because the Copilot
capture hook created sessions passively and never called `check_in_worktree` — see decision 4
above and ticket `40349f3f`.

An earlier sampled estimate had claimed roughly 18/30 branch coverage; the real-store dry-run
above refuted it. The dry-run measurement is authoritative, not the sample — this correction is
recorded here as exactly the kind of finding this spec exists to preserve, per decision 3 above.

## Open items

- The backfill has been implemented and dry-run **only**. It has not been executed in write mode
  against the real `.session` store, and the user has not authorized that write. No later reader
  should assume historical session-ticket linkage has actually been repaired in the live store.

## Related tickets

- `.ticket/tickets/bba9b313-ff13-4fd1-91d4-6485a6c2f4de/ticket.toml` — Session API
  `sessions_for_ticket` query capability (api + cli + mcp).
- `.ticket/tickets/33463861-ffba-4ead-905e-5d867b707936/ticket.toml` — dogfood run against
  ticket `06cfe998`.
- `.ticket/tickets/7c74f2fe-2bfd-477c-847e-bc02200a4819/ticket.toml` — new
  `context-enrichment.agent.md` template.
- `.ticket/tickets/1ff57502-ad4e-4c40-a852-18752c18f44c/ticket.toml` — deferred Ticket API
  inverse query (backlog).
- `.ticket/tickets/2b75bac2-ff14-43c3-8e87-1e801772f309/ticket.toml` — `sessions_for_ticket` was
  inert against the real store; capture linkage + backfill decision (in-review).
- `.ticket/tickets/e4d4c667-6d51-41c2-bd73-098911def78e/ticket.toml` — `sessions_for_ticket`
  aborted the whole scan on a corrupt store entry (in-review).
- `.ticket/tickets/40349f3f-8d04-4bf6-9241-b79425c10a97/ticket.toml` — capture hook did not
  record worktree assignment, root cause of `2b75bac2` (in-review).
