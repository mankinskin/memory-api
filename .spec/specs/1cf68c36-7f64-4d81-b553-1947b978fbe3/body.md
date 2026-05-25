# Summary

`spec-api` should be able to generate spec document files from canonical snippet records the same way `rule-api` already generates markdown outputs from target configurations. The generation mechanism should not reuse `rule-api` by copying private rendering logic into `spec-api`; instead, the shared file-building path should be extracted behind a domain-agnostic abstraction.

## Problem

Today the document-generation path lives almost entirely in `rule-api`:

- hierarchical target configuration
- ordered snippet collection from the store
- duplicate detection across outline nodes
- generated markdown rendering with provenance comments
- generated-file bookkeeping and stale-output cleanup
- newline-preserving rewrites

`spec-api` only offers direct folder CRUD over `spec.toml`, `body.md`, and `sections/`. That is enough for authored specs, but not for ubiquitous snippets that should be reused across many specs.

## Proposed model

### Shared builder seam

Introduce a shared builder layer under `memory-viewers/memory-api/crates/` that owns:

- rendering of generated outputs from ordered snippet blocks
- provenance-comment formatting for generated markdown files
- generated-output preparation that preserves the existing newline convention on rewrite

This layer should be generic over the domain adapter that supplies snippet records and any domain-specific metadata.

### Domain adapters

- `rule-api` continues to own `rule-targets.yaml`, rule filters, ordered collection, duplicate detection, explain output, and generated-target bookkeeping.
- `spec-api` owns how generated content is attached to a spec folder, such as `body.md`, `sections/*.md`, or future generated artifacts.
- Neither domain should duplicate the shared snippet-rendering or newline-safe rewrite logic.

### Spec document generation

A `spec-api` generation adapter may source its snippets from canonical rule-like content, but the spec contract is about generated spec documents rather than about rule storage itself. The first supported outcome may be limited to generating `body.md` or selected section files for a spec.

The current implementation covers both `body.md` generation and named `sections/*.md` generation through shared snippet rendering and newline-preserving rewrites.

## Non-goals

- Replacing authored `spec.toml` metadata with generated content.
- Moving all `rule-api` target configuration into `spec-api` in the first slice.
- Requiring `spec-api` to understand every `rule-api` filter or explain feature before generated spec documents are possible.

## Acceptance criteria

- A shared document-builder abstraction is documented as the common generation path for snippet-backed markdown files.
- `rule-api` reuses that abstraction for markdown rendering and output preparation without changing its observable output contract.
- `spec-api` gains a generated-document capability for `body.md` and selected section files.
- The design explicitly separates store querying, duplicate detection, and target bookkeeping from generic file-building behavior.
- The first implementation keeps deterministic ordering, duplicate rejection, provenance policy, and existing newline-preservation guarantees.

## Related specs

- `rule-api/workspaces`
- `rule-api/workspaces/nested-resolution`
- `spec-api`
- `spec-api/store`
