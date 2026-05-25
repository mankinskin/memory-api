# Problem

`rule-api` already knows how to collect ordered snippet content from a database-backed store, render generated markdown outputs, and rewrite files without losing the current newline convention. `spec-api` does not have an equivalent document-generation surface today, so planned spec narratives and repeated requirement blocks still have to be copied into `body.md` or `sections/` by hand.

This creates duplication across specs when the same guidance, requirement templates, or acceptance-criteria snippets should appear in several spec documents.

## Desired outcome

Use canonical snippet content to generate parts of spec documents the same way `rule-api` generates README and agent documents from rule targets, but without coupling `spec-api` directly to `rule-api` internals.

## Proposed direction

- Extract the generic snippet-rendering and newline-safe file-building path from `rule-api` into a reusable crate or core module.
- Keep domain-specific query/filter logic, duplicate detection, and generated-target bookkeeping in adapters.
- Refactor `rule-api` to use the shared builder first.
- Add a `spec-api` adapter that can generate spec `body.md` or section files from canonical snippet records.

## Acceptance criteria

- A shared document-builder abstraction exists for snippet rendering, provenance-comment formatting, and generated output preparation.
- `rule-api` target generation uses the shared abstraction for markdown rendering and output preparation without regressing target ordering, duplicate detection, provenance comments, or newline preservation.
- `spec-api` can generate spec document content from canonical snippet records without duplicating `rule-api` rendering internals.
- Focused tests cover at least one `rule-api` generation path and one `spec-api` generation path through the shared builder.
- The owning `spec-api` spec documents the cross-domain generation contract and the abstraction boundary between domain adapters and the shared builder.

## Current implementation

- The shared builder lives in `crates/memory-api/src/generated_markdown.rs`.
- `rule-api` now delegates markdown rendering and newline-preserving output preparation to that shared module while keeping target composition, duplicate detection, and generated-target bookkeeping local.
- `spec-api` now exposes generated document rendering plus `SpecStore::update_generated_body` and `SpecStore::update_generated_section` for `body.md` and named section generation from canonical snippets.

## Validation

- `cargo test -p memory-api generated_markdown -- --nocapture`
- `cargo test -p rule-api render::tests -- --nocapture`
- `cargo test -p spec-api update_generated_ -- --nocapture`
