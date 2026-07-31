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

### Shared builder seam

Introduce a shared builder layer under `memory-api/crates/` that owns:

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

## Next slice: rule-target-backed spec artifacts

The next planned slice should let a spec consume `rule-api` target outputs without teaching `spec-api` to evaluate raw rule filters itself.

- A spec-local descriptor, `generated.toml`, maps `body.md` and named `sections/*.md` artifacts to target names.
- `rule-api` should remain the only layer that evaluates `rule-targets.yaml`, imports canonical prose, and composes ordered snippet outputs.
- A `spec-cli` sync command or equivalent orchestration layer should resolve the descriptor, render each target, write the corresponding spec artifact through `spec-api`, and refresh spec-facing bookkeeping afterward.
- The first migration slice should prove the workflow on one real spec and document how to move reusable prose into canonical rules without moving `spec.toml` ownership.

### Descriptor contract

The implemented descriptor format is a spec-local `generated.toml` file with one optional `body` entry plus any number of named section entries:

```toml
[body]
config = "rule-targets.yaml"
target = "body"

[sections.requirements]
config = "rule-targets.yaml"
target = "requirements"

[sections.design]
config = "spec/rule-targets.yaml"
target = "design"
```

The descriptor is intentionally artifact-oriented rather than rule-filter-oriented:

- `spec-api` stores and validates the mapping from a spec artifact to a target name.
- `rule-api` still owns the meaning of `config`, target lookup, imports, filters, and outline composition.
- Section keys are normalized to section artifact names and remain confined to `sections/*.md`.

### Descriptor validation

The current descriptor implementation rejects:

- missing or blank `config` values
- missing or blank `target` values
- duplicate section aliases such as `requirements` and `requirements.md`
- section names that attempt to escape `sections/*.md`

### Planned acceptance criteria for the next slice

- Specs can declare generated `body.md` and named section artifacts through an explicit artifact-to-target mapping.
- Syncing generated spec artifacts uses a spec-owned workflow rather than writing files behind `spec-api`'s back.
- The design keeps `spec.toml` authored and local while generated prose moves into canonical rules and target outputs.
- At least one real spec migration validates the authoring and regeneration workflow end to end.

### Implementation status

The `spec-cli` orchestration slice is now implemented behind `spec sync-generated <spec-id>`.

- This pilot spec maps `body.md` and `sections/migration-workflow.md` through `generated.toml` so both artifacts regenerate from rule targets instead of hand-edited markdown.
- The command loads a spec's `generated.toml` descriptor through `spec-api`.
- It resolves the owning workspace from the indexed spec path and matching registered scan root so nested workspaces use the correct local `rule-targets.yaml` and `.rule` store instead of falling back to an ancestor checkout.
- It opens `rule-api`, evaluates each declared target, rewrites `body.md` and any declared `sections/*.md` through the generated-document helpers in `spec-api`, and then refreshes spec search/body bookkeeping through the normal `SpecStore::update` path.

### Validation status

- Passing: `cargo test -p spec-cli sync_generated -- --nocapture`
- Passing pilot flow: `rule explain-target --config rule-targets.yaml --target spec-api-generated-documents-body --json`, `spec sync-generated 1cf68c36-7f64-4d81-b553-1947b978fbe3 --workspace-root . --json`, `spec get 1cf68c36-7f64-4d81-b553-1947b978fbe3 --full --json`, `spec search "migration-workflow" --limit 5 --json`, and `spec refs 1cf68c36-7f64-4d81-b553-1947b978fbe3 validate --workspace-root . --json`.
- Passing broader suite: `cargo test -p spec-cli`.

## Related tickets

- [f4b0be64 Generate spec documents from canonical snippets via shared builder](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/f4b0be64-a2f5-4cb5-a476-b2b921d6ff02/ticket.toml)
- [a5fe4c58 Adopt rule targets for generated spec artifacts](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/a5fe4c58-f59c-4d97-8ee6-3447724b5fac/ticket.toml)
- [09641443 Add spec-local target mapping for generated spec artifacts](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/09641443-a8f2-479d-85cb-ea44a963595b/ticket.toml)
- [b2ef1de1 Add spec sync-generated orchestration for rule-target-backed artifacts](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/b2ef1de1-5801-47c6-97c6-e3c5cd8d7dae/ticket.toml)
- [7f869c33 Pilot migration for rule-target-backed spec artifacts](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-api/.ticket/tickets/7f869c33-15ff-4959-8161-731844eef21b/ticket.toml)

## Related specs

- `rule-api/workspaces`
- `rule-api/workspaces/nested-resolution`
- `spec-api`
- `spec-api/store`
