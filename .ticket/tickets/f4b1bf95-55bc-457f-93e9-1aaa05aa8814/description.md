# Goal

Make `spec refs validate` catch references to non-existent test/validation guard specs instead of reporting a clean pass.

# Problem (evidence + root cause)

In session `beca8ec5`, specs `e302d4c3` and `9e823b76` both link `.test/default/specs/val-viewer-first-batch.json` as their required aggregate guard, and `spec refs validate` "passed" on both. That file does not exist anywhere in the tree.

Root cause (verified): `validate_refs` (`memory-api/crates/spec-api/src/code_ref.rs:49`) validates **only** `CodeRef` entries (file / symbol / line-range). It never inspects guards at all. Guards live under a `## Guards` markdown heading and are parsed separately by `parse_guards_from_markdown` (`memory-api/crates/spec-api/src/verification.rs:24`), then resolved through test-api only inside `recompute_spec_verified_state`. So `spec refs validate` passes a dangling guard because it structurally ignores guards — not because it resolves them incorrectly.

# Solution Design

1. In the `spec refs validate` path (spec-api store + CLI + MCP), after `validate_refs`, also call `parse_guards_from_markdown(&body)` to extract guard ids.
2. Resolve each guard id through test-api: `test_store.get_spec(guard_id)`. Guards are ids (e.g. `val-viewer-first-batch`), not paths; the observed `.test/default/specs/val-viewer-first-batch.json` is the on-disk form of that id.
3. Introduce a `GuardValidation { guard_id, resolved: bool, message: Option<String> }` result and include it in the refs-validate output alongside `RefValidation`. A missing guard is a validation failure with the guard id and expected `.test` path.
4. Refs-validate exit/status must be non-clean when any guard is unresolved.

# Acceptance Criteria

1. A spec linking a non-existent `.test` guard fails `refs validate`, naming the missing guard id and expected path.
2. A spec linking an existing guard passes.
3. Guard resolution goes through test-api (`get_spec`), not raw filesystem path existence.
4. Focused tests cover both the dangling-guard and resolvable-guard cases.

# Traceability

- Observed in specs `e302d4c3-c24f-4778-bef0-453d3c1997bb` and `9e823b76-cd60-4689-b772-649ebb3a34a1`.