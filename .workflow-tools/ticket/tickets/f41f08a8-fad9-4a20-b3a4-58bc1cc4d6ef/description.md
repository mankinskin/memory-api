## Bug: Panic in `integration::edge_case_tests::edge_repeated_single_char` (RC-3)

### Status update (2026-07-06)
The **public `context-cli` integration test now passes** because `context-api::read_sequence` was redirected through the corrected public exact-root insert path before reading the root back.

### Important distinction
This session did **not** fix the underlying repeated/overlap algorithm inside `context-read` / `context-trace`.
Instead, it removed the panic from the public `ReadSequence` API surface.

### What is now true
- `cargo test -p context-cli --test cli_integration edge_repeated_single_char` passes.
- Public `read_sequence("aaaa")` returns a root of width 4 through the `context-api` wrapper.

### What is still unresolved
The deeper repeated/overlap engine bug remains in `context-read` unit tests. The current workspace failures still include repeated/infix/overlap cases such as:
- `tests::linear::repetition_aabbaabb`
- `tests::ngrams_validation::validate_mixed_pattern`
- `tests::overlapping::complex_abcabababcaba`
- `tests::read::read_repeating_known1`

### Revised scope
Treat this ticket as the **underlying engine bug** rather than only the public integration symptom. The public symptom is masked/fixed at the wrapper layer, but the core repeated-overlap implementation is still broken.

### Next fix direction
Continue inside `context-read` / `context-trace`:
- atom-anchor overlap detection in `ExpansionCtx`
- repeated-pattern decomposition symmetry
- width/decomposition consistency for repeated-char and infix/overlap cases

### Current acceptance note
The original public integration reproduction is now green, but the ticket should remain open until the underlying repeated-overlap engine failures are resolved and the related ignored tests can be re-enabled.

### Scope note for batch-2 (`f2d8f807`)
This ticket is an acknowledged blocker dependency and is intentionally out of scope for the current public execution-surface pass.