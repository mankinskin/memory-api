# Problem

The shared generated-markdown builder and `spec-api` generated body/section update paths now exist, but there is still no end-to-end workflow for a spec to declare that `body.md` or `sections/*.md` should come from `rule-api` targets. Without that orchestration layer, generated spec artifacts remain an internal API rather than a supported authoring path.

## Desired outcome

Specs can opt into generated artifacts through an explicit descriptor and a supported sync workflow, while `spec-api` continues to own spec identity and `rule-api` continues to own target composition.

## Proposed direction

- Reuse `f4b0be64` as the completed foundation for shared rendering and spec artifact writes.
- Add a spec-local descriptor that maps `body.md` and named section files to `rule-targets.yaml` targets.
- Add a sync command that resolves the descriptor, renders the target outputs, writes the corresponding spec artifacts, and refreshes spec bookkeeping.
- Prove the workflow by migrating one real spec and documenting the migration path.

## Acceptance criteria

- The descriptor format, sync workflow, and pilot migration each have a concrete child ticket.
- This tracker depends on the shared-builder ticket plus the newly created child tickets for descriptor, sync, and migration work.
- The related spec documents the artifact-to-target boundary and the migration path before implementation starts.
