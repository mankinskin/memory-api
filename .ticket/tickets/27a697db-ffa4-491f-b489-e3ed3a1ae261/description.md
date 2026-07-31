## Problem

`cargo test -p ticket` does not compile. [memory-api/crates/ticket/src/lib.rs](memory-api/crates/ticket/src/lib.rs#L10) contains a `#[cfg(test)]` assertion `let _ = storage::TicketStore::default;`, but `TicketStore` has never implemented `Default`. Live run yields:

```
error[E0599]: no associated function or constant named `default` found for struct `TicketStore`
```

`grep` for `impl Default for TicketStore` across the ticket-api crate returns zero matches. Broken since `ae09e93`.

`cargo build -p ticket` (non-test) succeeds — the break is isolated to the test assertion, so production builds are unaffected. But the crate's test suite is red repo-wide.

## Context

Discovered during the epic-session review (2026-07-27). A sub-agent previously commented out this assertion to get past a build error and labelled it "unrelated"; the audit restored it. The assertion is currently live, not commented out.

## Acceptance criteria

1. `cargo test -p ticket` compiles and runs.
2. Either `TicketStore` implements `Default` (with a documented, sane default store root), or the assertion is removed with a recorded rationale for why the constructor contract does not require `Default`.
3. If the assertion is removed, a replacement assertion covers whatever construction contract is actually intended.
4. No production-code behavior change beyond the chosen option.

## Non-goals

- Refactoring `TicketStore` construction more broadly.
- Touching any other crate's test setup.