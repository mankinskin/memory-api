## G-B — Rust code-design policy

Author canonical Rust code-design policy as rule-api content and drive the first retro-fixes. Learnings to encode:

1. **Typed errors** — classify errors with the type system; no stringly-typed markers inside catch-all variants. Prefer dedicated error enum variants; reserve `Domain(String)` for genuinely open-ended cases only.
2. **Trait-based contracts** — express interface contracts with traits and static dispatch instead of runtime validation plumbing (`validate_*` helpers checking invariants that types could guarantee).
3. **Trait inheritance / generic typing** — use supertraits and generics for truly generic contracts.
4. **Exemplars** — mine context-stack crates (context-trace/search/insert/read/api) for idiomatic trait-heavy patterns and cite them.

## Child retro-fixes
- Fix `MoveError::Domain` + `MoveJournal::INTEROP_CONTRACT_MARKER` stringly-typed marker → typed variant.
- Convert runtime `validate_interoperability_contract` interoperability checks → trait-based contract where feasible.

## Anchor (first concrete instances)
- INTEROP db9bad13-ae43-4300-8037-7165c0e9a7b0 (contract owner)
- TRACKER 6e72756f-11c6-405f-8d74-0ab608172871 (umbrella)

## Acceptance criteria
- Rust code-design policy rule(s) authored and queryable.
- Two retro-fix child tickets exist and link the INTEROP/TRACKER lineage.