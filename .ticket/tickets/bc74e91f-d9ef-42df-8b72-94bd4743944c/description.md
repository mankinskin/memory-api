## Problem (Finding F1, discovered during f9e70385/3d952036 rework review on 2026-07-30)

On a single combined call that both transitions a ticket `--to-state planned` and writes `--description ... --description-mode ...` in the same request, the newly materialized `objective` part file is written with the ticket's **pre-call** description content (often empty), while `description.md` receives the **new** text. The two diverge immediately after a legitimate, non-rejected call.

Root cause (read from code during review):
- `TicketFs::apply_plan_freeze` (memory-api/crates/ticket-api/src/storage/ticket_fs.rs) materializes the `objective` part from `read_description` (the ticket's pre-call description) during the transition step.
- `TicketStore::apply_manifest_update` (memory-api/crates/ticket-api/src/storage/store.rs) calls `TicketFs::write_description` **after** the transition-path loop, writing the new text only to `description.md`, never back into the just-created `objective` part file.

## Reproduction
`ticket update <id> --to-state planned --description "combined write content" --description-mode replace` → resulting `objective` part file = 0 bytes; `description.md` = correct new content.

## Suggested fix directions (not prescriptive)
(a) Route freeze-time `objective` materialization to occur *after* the description write within the same call, or
(b) Re-read `description.md` at freeze time only if a description write is also present in the same call.

## Scope
Not an AC violation for f9e70385 or 3d952036 (neither ticket's ACs assert objective-part *content* correctness on a combined call). Confirmed real via live CLI repro during the Review Agent's pass on tickets f9e70385/3d952036 (2026-07-30).

## Acceptance Criteria
1. A combined freeze+description-write call results in the `objective` part file containing the same content as `description.md` immediately after the call (byte-identical for the description portion).
2. Regression test added reproducing the exact combined-call shape from this ticket's repro section, asserting objective-part content matches the written description.
3. No regression to the freeze-bypass fix from f9e70385 (separate-call write-after-freeze must remain rejected).
4. Test-api evidence recorded and linked to this ticket id.
