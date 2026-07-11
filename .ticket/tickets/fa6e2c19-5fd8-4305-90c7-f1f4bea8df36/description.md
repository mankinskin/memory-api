## G-B retro-fix 2 — Trait-based interoperability contract instead of runtime validation

Convert the runtime `validate_interoperability_contract` / `interoperability_gaps` plumbing into a trait-based contract where the type system can enforce the shared minimum interoperability set plus artifact-specific required extensions.

## Task
- Define a trait (e.g. `InteroperableArtifact`) expressing the shared minimum contract, with supertrait/associated bounds for artifact-specific required extensions.
- Replace runtime gap-collection with compile-time-enforced trait bounds where feasible; keep runtime checks only for genuinely dynamic data.
- Apply across the five artifact classes (validation executions, benchmark records, log captures, runtime sessions, journal-backed operations).

## Reference exemplars
- context-stack crates (context-trace/search/insert/read/api) for idiomatic trait-heavy contract patterns.

## Lineage
Second concrete instance of the G-B trait-contract policy. Anchors: INTEROP db9bad13, TRACKER 6e72756f.