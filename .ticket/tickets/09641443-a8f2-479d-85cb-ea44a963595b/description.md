# Problem

`spec-api` has generated artifact write paths, but a spec folder has no explicit way to declare which `rule-api` target should produce `body.md` or any named section file. If that mapping is left implicit, authors have no durable contract for generated spec artifacts.

## Desired outcome

A spec can declare generated artifacts without embedding raw rule filters or `rule-targets.yaml` semantics into `spec-api` core.

## Proposed direction

- Introduce a spec-local descriptor, for example `generated.toml`, that maps `body` and named sections to target names.
- Keep the descriptor at the artifact-to-target level instead of duplicating rule selection/filter logic inside `spec-api`.
- Preserve authored `spec.toml` as the source of truth for spec identity, hierarchy, and metadata.

## Acceptance criteria

- The descriptor format covers `body.md` and multiple named section files.
- The format documents how target configs are resolved and how generated artifacts remain confined to the spec folder contract.
- Validation rules reject ambiguous mappings, missing targets, or attempts to write outside `body.md` and `sections/*.md`.
- The owning spec records the descriptor contract and its separation from `rule-api` target evaluation.
