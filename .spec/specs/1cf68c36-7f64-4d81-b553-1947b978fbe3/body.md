<!-- spec-api:file generated=true -->

<!-- spec-api:entry id=16786c84-4e99-4041-8292-a962d907c541 slug=spec-api/generated-documents/summary/l1 -->
# Summary

`spec-api` should be able to generate spec document files from canonical snippet records the same way `rule-api` already generates markdown outputs from target configurations. The generation mechanism should not reuse `rule-api` by copying private rendering logic into `spec-api`; instead, the shared file-building path should be extracted behind a domain-agnostic abstraction.

<!-- spec-api:entry id=91b389d0-a9c9-458c-8aa8-d51627b629c7 slug=spec-api/generated-documents/summary/problem/l5 -->
## Problem

Today the document-generation path lives almost entirely in `rule-api`:

<!-- spec-api:entry id=7c113526-c4e7-42d4-8c61-b3c0905cbb9c slug=spec-api/generated-documents/summary/problem/l9 -->
- hierarchical target configuration
- ordered snippet collection from the store
- duplicate detection across outline nodes
- generated markdown rendering with provenance comments
- generated-file bookkeeping and stale-output cleanup
- newline-preserving rewrites

<!-- spec-api:entry id=aeeb9889-1972-4b3e-8cdd-50f64bcc5b6b slug=spec-api/generated-documents/summary/problem/l16 -->
`spec-api` only offers direct folder CRUD over `spec.toml`, `body.md`, and `sections/`. That is enough for authored specs, but not for ubiquitous snippets that should be reused across many specs.

<!-- spec-api:entry id=cf4a19a7-6fc9-4d1a-920a-7e9734bf02fc slug=spec-api/generated-documents/summary/proposed-model/shared-builder-seam/l20 -->
### Shared builder seam

Introduce a shared builder layer under `memory-viewers/memory-api/crates/` that owns:

<!-- spec-api:entry id=c25c3b1c-5615-4dd5-aaa2-c39ea6bbac50 slug=spec-api/generated-documents/summary/proposed-model/shared-builder-seam/l24 -->
- rendering of generated outputs from ordered snippet blocks
- provenance-comment formatting for generated markdown files
- generated-output preparation that preserves the existing newline convention on rewrite

<!-- spec-api:entry id=7f4e668c-3037-4d51-9954-bead0f4eb25e slug=spec-api/generated-documents/summary/proposed-model/shared-builder-seam/l28 -->
This layer should be generic over the domain adapter that supplies snippet records and any domain-specific metadata.

<!-- spec-api:entry id=7e6f64bb-5b78-47fc-9b26-e1f3fb787d20 slug=spec-api/generated-documents/summary/proposed-model/domain-adapters/l30 -->
### Domain adapters

- `rule-api` continues to own `rule-targets.yaml`, rule filters, ordered collection, duplicate detection, explain output, and generated-target bookkeeping.
- `spec-api` owns how generated content is attached to a spec folder, such as `body.md`, `sections/*.md`, or future generated artifacts.
- Neither domain should duplicate the shared snippet-rendering or newline-safe rewrite logic.

<!-- spec-api:entry id=0ff22ee2-e5a5-4ecf-9f3b-299dba241074 slug=spec-api/generated-documents/summary/proposed-model/spec-document-generation/l36 -->
### Spec document generation

A `spec-api` generation adapter may source its snippets from canonical rule-like content, but the spec contract is about generated spec documents rather than about rule storage itself. The first supported outcome may be limited to generating `body.md` or selected section files for a spec.

<!-- spec-api:entry id=c92cfd00-ffaf-4951-8e65-da75a56163a6 slug=spec-api/generated-documents/summary/proposed-model/spec-document-generation/l40 -->
The current implementation covers both `body.md` generation and named `sections/*.md` generation through shared snippet rendering and newline-preserving rewrites.

<!-- spec-api:entry id=f8b3cea0-fb0b-455c-bf1f-806b695482f1 slug=spec-api/generated-documents/summary/non-goals/l42 -->
## Non-goals

- Replacing authored `spec.toml` metadata with generated content.
- Moving all `rule-api` target configuration into `spec-api` in the first slice.
- Requiring `spec-api` to understand every `rule-api` filter or explain feature before generated spec documents are possible.

<!-- spec-api:entry id=21c67de3-06bb-42ca-8819-b82ef2e4a531 slug=spec-api/generated-documents/summary/acceptance-criteria/l48 -->
## Acceptance criteria

- A shared document-builder abstraction is documented as the common generation path for snippet-backed markdown files.
- `rule-api` reuses that abstraction for markdown rendering and output preparation without changing its observable output contract.
- `spec-api` gains a generated-document capability for `body.md` and selected section files.
- The design explicitly separates store querying, duplicate detection, and target bookkeeping from generic file-building behavior.
- The first implementation keeps deterministic ordering, duplicate rejection, provenance policy, and existing newline-preservation guarantees.

<!-- spec-api:entry id=2c4f3957-5377-4cf0-9050-512c7a26f2f7 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/l56 -->
## Next slice: rule-target-backed spec artifacts

The next planned slice should let a spec consume `rule-api` target outputs without teaching `spec-api` to evaluate raw rule filters itself.

<!-- spec-api:entry id=23dff709-4a4f-4a53-b767-4feb9e32c4bf slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/l60 -->
- A spec-local descriptor, `generated.toml`, maps `body.md` and named `sections/*.md` artifacts to target names.
- `rule-api` should remain the only layer that evaluates `rule-targets.yaml`, imports canonical prose, and composes ordered snippet outputs.
- A `spec-cli` sync command or equivalent orchestration layer should resolve the descriptor, render each target, write the corresponding spec artifact through `spec-api`, and refresh spec-facing bookkeeping afterward.
- The first migration slice should prove the workflow on one real spec and document how to move reusable prose into canonical rules without moving `spec.toml` ownership.

<!-- spec-api:entry id=42ad6525-7fe1-4739-9d63-58e3efafe12e slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/descriptor-contract/l65 -->
### Descriptor contract

The implemented descriptor format is a spec-local `generated.toml` file with one optional `body` entry plus any number of named section entries:

<!-- spec-api:entry id=a59e9e3c-dfbd-45ba-a3ba-5c91d7a17dde slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/descriptor-contract/l69 -->
```toml
[body]
config = "rule-targets.yaml"
target = "body"

<!-- spec-api:entry id=c765faa0-cec1-4196-8757-38b4de8d181b slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/descriptor-contract/l74 -->
[sections.requirements]
config = "rule-targets.yaml"
target = "requirements"

<!-- spec-api:entry id=a281bc55-4795-4c5b-be04-f7e7bcc477a0 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/descriptor-contract/l78 -->
[sections.design]
config = "spec/rule-targets.yaml"
target = "design"
```

<!-- spec-api:entry id=65204c60-650a-4f69-9fe5-852434df3dc4 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/descriptor-contract/l83 -->
The descriptor is intentionally artifact-oriented rather than rule-filter-oriented:

<!-- spec-api:entry id=9a3f38e6-177a-46ea-9243-f6c2646dd072 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/descriptor-contract/l85 -->
- `spec-api` stores and validates the mapping from a spec artifact to a target name.
- `rule-api` still owns the meaning of `config`, target lookup, imports, filters, and outline composition.
- Section keys are normalized to section artifact names and remain confined to `sections/*.md`.

<!-- spec-api:entry id=6a994ba9-2014-4984-9a18-7283353802e8 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/descriptor-validation/l89 -->
### Descriptor validation

The current descriptor implementation rejects:

<!-- spec-api:entry id=c7803a26-fb05-46c2-a8a9-4681e645a988 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/descriptor-validation/l93 -->
- missing or blank `config` values
- missing or blank `target` values
- duplicate section aliases such as `requirements` and `requirements.md`
- section names that attempt to escape `sections/*.md`

<!-- spec-api:entry id=42953cc1-9983-4074-885b-eec548816864 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/planned-acceptance-criteria-for-the-next-slice/l98 -->
### Planned acceptance criteria for the next slice

- Specs can declare generated `body.md` and named section artifacts through an explicit artifact-to-target mapping.
- Syncing generated spec artifacts uses a spec-owned workflow rather than writing files behind `spec-api`'s back.
- The design keeps `spec.toml` authored and local while generated prose moves into canonical rules and target outputs.
- At least one real spec migration validates the authoring and regeneration workflow end to end.

<!-- spec-api:entry id=97647246-e24e-48f8-970c-8e2e464f2346 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/implementation-status/l105 -->
### Implementation status

The `spec-cli` orchestration slice is now implemented behind `spec sync-generated <spec-id>`.

<!-- spec-api:entry id=b92abfc0-7b4b-4815-b64c-05a8e1d34009 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/implementation-status/l109 -->
- This pilot spec maps `body.md` and `sections/migration-workflow.md` through `generated.toml` so both artifacts regenerate from rule targets instead of hand-edited markdown.
- The command loads a spec's `generated.toml` descriptor through `spec-api`.
- It resolves the owning workspace from the indexed spec path and matching registered scan root so nested workspaces use the correct local `rule-targets.yaml` and `.rule` store instead of falling back to an ancestor checkout.
- It opens `rule-api`, evaluates each declared target, rewrites `body.md` and any declared `sections/*.md` through the generated-document helpers in `spec-api`, and then refreshes spec search/body bookkeeping through the normal `SpecStore::update` path.
- `rule-cli sync-targets` and `rule-mcp` target generation now treat `file_kind: spec-doc` targets as spec-owned artifacts: they open the owning `spec-api` store, write the corresponding `.spec/specs/**/{body,sections/*.md}` file through `spec-api`, and stop using `generated/spec-docs/**` mirror files as the canonical output surface.

<!-- spec-api:entry id=bfcc40c9-422f-4048-b26b-21d8c22129b0 slug=spec-api/generated-documents/summary/next-slice-rule-target-backed-spec-artifacts/validation-status/l114 -->
### Validation status

- Passing: `cargo test -p spec-cli sync_generated -- --nocapture`
- Passing pilot flow: `rule explain-target --config rule-targets.yaml --target spec-api-generated-documents-body --json`, `spec sync-generated 1cf68c36-7f64-4d81-b553-1947b978fbe3 --workspace-root . --json`, `spec get 1cf68c36-7f64-4d81-b553-1947b978fbe3 --full --json`, `spec search "migration-workflow" --limit 5 --json`, and `spec refs 1cf68c36-7f64-4d81-b553-1947b978fbe3 validate --workspace-root . --json`.
- Passing follow-up slice: `cargo test -p rule-cli sync_targets_writes_spec_doc_targets_into_spec_entries -- --nocapture`, `cargo build -p rule-mcp`, and `./target/debug/rule.exe --workspace-root . sync-targets --config rule-targets.yaml --json` now report `.spec/specs/**` outputs for `spec-doc` targets instead of `generated/spec-docs/**` mirror files.
- Passing broader suite: `cargo test -p spec-cli`.

<!-- spec-api:entry id=152dcba7-8bca-4231-b7e5-1970de77b539 slug=spec-api/generated-documents/summary/related-tickets/l119 -->
## Related tickets

- [f4b0be64 Generate spec documents from canonical snippets via shared builder](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-viewers/memory-api/.ticket/tickets/f4b0be64-a2f5-4cb5-a476-b2b921d6ff02/ticket.toml)
- [a5fe4c58 Adopt rule targets for generated spec artifacts](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-viewers/memory-api/.ticket/tickets/a5fe4c58-f59c-4d97-8ee6-3447724b5fac/ticket.toml)
- [09641443 Add spec-local target mapping for generated spec artifacts](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-viewers/memory-api/.ticket/tickets/09641443-a8f2-479d-85cb-ea44a963595b/ticket.toml)
- [b2ef1de1 Add spec sync-generated orchestration for rule-target-backed artifacts](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-viewers/memory-api/.ticket/tickets/b2ef1de1-5801-47c6-97c6-e3c5cd8d7dae/ticket.toml)
- [7f869c33 Pilot migration for rule-target-backed spec artifacts](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-viewers/memory-api/.ticket/tickets/7f869c33-15ff-4959-8161-731844eef21b/ticket.toml)
- [87a35ccb Route spec-doc targets through spec-owned generation](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/context-engine/memory-viewers/memory-api/.ticket/tickets/87a35ccb-d91c-4ce8-93b3-e150bb5afe1d/ticket.toml)

<!-- spec-api:entry id=ac096ea4-df1c-40f6-9fdf-35ea461b6a69 slug=spec-api/generated-documents/summary/related-specs/l127 -->
## Related specs

- `rule-api/workspaces`
- `rule-api/workspaces/nested-resolution`
- `spec-api`
- `spec-api/store`
