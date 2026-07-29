## Objective

Split the existing ticket corpus's monolithic descriptions into typed parts, moving only content that can be confidently classified and leaving everything else in `objective` verbatim, with a dry-run report before any write.

## Requirements

- Dry-run and apply are separate steps. Apply refuses to run without a preceding dry-run report.
- Recognised headings move to their typed part: `## Review` → `review`, `## Status` → `notes`, `## Validation` → `validation`, `## Handoff` → `notes`.
- Heading matching is case-insensitive and tolerates trailing text (`## Review 2026-07-29`).
- All content that is not a confident match stays in `objective` byte-for-byte: unrecognised headings, mid-description asides, prose fitting no core kind. Nothing is guessed.
- Total content is conserved: concatenating all resulting parts in order reproduces the original description exactly.
- Migration writes one `notes` part per matched heading and never merges multiple headings into a single part.
- The dry-run report lists, per affected ticket: ticket id, title, which sections move to which kind, and how many lines stay in `objective`.
- Migration writes a history revision per ticket so `undo` reverses it.
- Migration is idempotent: a second run over a migrated ticket is a no-op.
- There is no bypass around the freeze contract from f9e70385: if a migrated ticket is already `planned`, the workflow first transitions it back to a pre-`planned` state, then splits the description, then re-enters `planned` so the ticket is re-frozen and a new plan revision is cut.

## Design

Content conservation is the correctness property that matters. The heuristic will mis-classify some sections; the mitigation is that a miss leaves content in `objective` where it is today, never in a wrong part and never deleted. That makes the failure mode "some tickets stay messy" rather than "some content is lost".

This ticket depends on f9e70385, so migration must use the normal state-transition path to step planned tickets back to a pre-`planned` state before any split is written. That re-freeze on return to `planned` is how the migration creates a fresh plan revision rather than bypassing the freeze contract.

Over 50 tickets carry these headings today. The largest descriptions are 61f78a57 (1286 lines), f95969ba (1280), 19990e37 (1088) — use these three as the migration fixtures, since they exercise the widest heading variety.

The tickets in this track are authored under the fixed heading convention, so they double as clean positive fixtures for the classifier.

## Implementation Steps

1. Add a dry-run planner in `memory-api/crates/ticket-api/src/storage/store.rs` that scans `description.md`, classifies headings, and emits a per-ticket split report without writing.
2. Add the apply path in `memory-api/crates/ticket-api/src/storage/store.rs` (or the dedicated migration tool layer) so it refuses to run unless it receives the exact dry-run report it just produced.
3. Reuse the new part manifest model from 5a3d152c to create one `notes` part per matched heading and keep unmatched text in the implicit `objective` part byte-for-byte.
4. Make the migration path state-aware in `memory-api/crates/ticket-api/src/storage/store/lifecycle.rs` so a `planned` ticket is first moved back to a pre-`planned` state, then split, then re-entered into `planned` to re-freeze.
5. Append a history revision through `memory-api/crates/ticket-api/src/storage/ticket_fs.rs::append_history` for each migrated ticket so `undo` restores the single-description layout.
6. Add regression tests under `memory-api/crates/ticket-api/src/storage/tests/update_regression_tests.rs` and the migration tool tests for dry-run enforcement, idempotence, freeze-compatible migration, and content conservation.

## Examples

Dry-run output shape:

```
61f78a57  Session store: populate tool-metrics
  → review      2 sections   (## Review, ## Review 2026-07-12)   118 lines
  → validation  1 section    (## Validation Results)              44 lines
  → notes       3 sections   (## Status, ## Handoff, ## Update)  201 lines
  → objective   unchanged                                        923 lines
```

## Acceptance Criteria

1. For every migrated ticket, concatenating the resulting parts in manifest order reproduces the pre-migration `description.md` byte-for-byte.
2. Apply without a preceding dry-run is refused.
3. The three largest tickets (61f78a57, f95969ba, 19990e37) migrate and satisfy criterion 1.
4. A heading variant (`## review`, `## Review 2026-07-29`) is recognised; an unrecognised heading's section remains in `objective`.
5. Re-running migration over a migrated ticket changes nothing on disk.
6. `undo` after migration restores the original single-description layout.
7. A ticket in `planned` migrates successfully only by stepping back to a pre-`planned` state first, then re-entering `planned` so freeze state is re-applied.
8. A post-migration audit reports zero confidently-classifiable review/status/validation/handoff headings remaining in any `objective` part.
9. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- code: memory-api/crates/ticket-api/src/storage/ticket_fs.rs
- fixture: memory-api/.ticket/tickets/61f78a57-6896-4ad7-9daa-0e9e805aa397
- fixture: memory-api/.ticket/tickets/f95969ba-c797-42d2-b6bc-9265a5fb4cf0
- fixture: memory-api/.ticket/tickets/19990e37-b5c2-41bc-af39-d649559a8885