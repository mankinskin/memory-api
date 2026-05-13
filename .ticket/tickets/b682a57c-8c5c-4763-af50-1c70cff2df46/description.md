# Phase 5: Bug Ticket Enrichment

## Objective

Enrich bug tickets with structured validity tracking and reproduction status, replacing the informal confidence emoji system.

## Background

The CLI already has a `ticket repro` command that records reproduction attempts with commit, timestamp, outcome, and optional command/note. It stores:
- `reproductions` — array of all attempts (history)
- `last_reproduced_at`, `last_reproduced_commit`, `last_reproduction_outcome` — denormalized latest attempt

This phase builds on that foundation by adding a `bug_validity` schema field and establishing a clear workflow.

## Bug Validity Model

### Schema field: `bug_validity`
- `not_confirmed` — Default for all bug tickets. No successful reproduction yet.
- `confirmed` — Latest reproduction attempt succeeded (`last_reproduction_outcome == "reproduced"`).

### Reproduction status (derived from existing `repro` data)
The reproduction state is determined from the existing `reproductions` array and denormalized fields:
- **No reproduction tested yet** — `reproductions` array is empty or missing
- **Reproduction failed** — Latest entry has `outcome != "reproduced"` (with commit + timestamp from `last_reproduced_commit` / `last_reproduced_at`)
- **Reproduction succeeded** — Latest entry has `outcome == "reproduced"` (with commit + timestamp)

A bug is valid (`bug_validity=confirmed`) if and only if the latest reproduction succeeded.

## Tasks

1. **Set `doc_category=bug-report`** on all bug tickets

2. **Set `bug_validity` field** on all bug tickets:
   - Tickets with existing `last_reproduction_outcome == "reproduced"` → `bug_validity=confirmed`
   - All others → `bug_validity=not_confirmed`

3. **Run `ticket repro`** for bug tickets where the agent bug-report file contains reproduction evidence:
   - Extract commit/timestamp from the report if available
   - Record via `ticket repro --id <id> --outcome reproduced|not-reproduced --commit <sha>`

4. **Attach research files as assets** where applicable:
   - Ticket `3125d4c5` (context-read 28 errors): attach `bug-reports/20251206_CONTEXT_READ_API_RESEARCH.md` → `assets/research/`

## Bug Tickets

| Ticket (short) | Title | Has Description | Reproduction Evidence |
|----------------|-------|-----------------|----------------------|
| `346573c1` | Cache root mismatch | Yes | In bug report |
| `3125d4c5` | Context-read 28 errors | Yes | Research file |
| `a4d0f88f` | SVG icons massive size | No | None |
| Other bug tickets in the store... | | | |

## Future Consideration

Consider automating `bug_validity` updates: when `ticket repro` is run and outcome is `reproduced`, automatically set `bug_validity=confirmed`. When outcome is `not-reproduced` or `fixed`, set `bug_validity=not_confirmed`. This could be a post-repro hook or built into the `repro` command itself.

## Verification

- `ticket list --field doc_category=bug-report` returns all bug tickets
- Bug tickets with reproduction evidence have `bug_validity=confirmed`
- Bug tickets without reproduction have `bug_validity=not_confirmed`
- `ticket list --with-repro` shows reproduction history for recorded attempts
