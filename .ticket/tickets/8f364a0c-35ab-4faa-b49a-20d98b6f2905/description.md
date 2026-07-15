# Goal

Implement the versioned structured reporter envelope and repository-native subprocess adapter specified by `test-api/browser-result-ingestion`.

# Scope

- Validate structured Playwright and wasm-pack result envelopes without parsing human console text.
- Map terminal/retry/infrastructure/capability outcomes to test-api passed, failed, and blocked executions.
- Persist source test, profile, transport, run/correlation identity, and artifact-manifest references with traceability links.
- Extend the typed test-api model only where existing provenance and links cannot preserve retry, profile, commit, correlation, or artifact identity.
- Add focused schema, retry, blocked-capability, provenance, and artifact-retention tests.

# Non-goals

- Embedding Node/Playwright in Rust.
- Replacing viewer-owned Playwright feature tests.

# Traceability

- Design spec: `.spec/specs/9e823b76-cd60-4689-b772-649ebb3a34a1/spec.toml`.
- Aggregate guard: `.test/default/specs/val-viewer-first-batch.json`.