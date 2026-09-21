## Scope
Finish fixing the remaining 7 audited discarded-`Result` `append_history` sites that are outside ticket-api. These are the counterparts to the 7 sites fixed in ticket 0c02b304.

## Sites to fix
- rule-api: `memory-api/crates/rule-api/src/store.rs` L475, L588, L679
- rule-api: `memory-api/crates/rule-api/src/store/generated_targets.rs` L129, L192
- spec-api: `memory-api/crates/spec-api/src/store.rs` L428, L683

## Pattern
Apply the same pattern used in ticket-api: wrap `TicketFs::append_history(...)` in `if let Err(error) = ... { tracing::error!(...) }` (log and continue, do not propagate), because by the time history is appended the manifest write has already committed and is the system of record. Propagating would report failure for a mutation that succeeded.

## Acceptance criteria
1. No `let _ = TicketFs::append_history(...)` remains in rule-api or spec-api source.
2. Each call site logs loudly on append failure.
3. Regression tests prove a failing `append_history` is no longer silently swallowed for at least one representative site per crate (rule-api, spec-api).
4. `cargo test -p rule-api --lib` and `cargo test -p spec-api --lib` pass.

## Related
- Blocks: 0c02b304-de04-4e29-818f-fb1e6797bdc0 (completes the repo-wide cleanup it scoped out)
