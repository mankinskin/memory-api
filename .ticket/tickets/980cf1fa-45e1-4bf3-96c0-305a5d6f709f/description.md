## Implementation Summary (2026-07-28)

Both halves of the fix landed:

**Reject-at-creation (AC1, AC2, AC5):** `validate_workflow_node_draft` in `memory-api/crates/session-api/src/store/config/runtime_workflow.rs` is now a `SessionStoreConfig` method (was a free function) so it can resolve against the `.test` store. It enforces, for `workflow_add_node`/`workflow_add_nodes`:
- `validation` requires a non-empty `validation_spec_id` that resolves via `TestStoreConfig::get_spec` against the sibling `.test` store (AC5 — a node UUID cannot be passed as a spec id).
- `ticket` requires a non-empty `ticket_urn`; `spec` requires a non-empty `spec_urn` (AC2, symmetric with validation).
- The batch tool (`session_workflow_add_nodes` / `workflow_add_nodes`) identifies the offending `nodes[index]` via the existing `indexed_workflow_error` wrapper.

**Repair surface (AC3):** added `workflow_update_node` (patch) and `workflow_remove_node` (delete) to `memory-api/crates/session-api/src/store/config/workflow.rs`, backed by a new `SessionWorkflowNodePatch` model type (`memory-api/crates/session-api/src/model/workflow.rs`, re-exported via `model.rs`/`lib.rs`). `workflow_update_node` merges the patch onto the existing node and re-validates it with the same creation rules before persisting, so a patch cannot introduce a new wedge. `workflow_remove_node` deletes the node and prunes any edges referencing it. Exposed at the MCP layer as `session_workflow_update_node` / `session_workflow_remove_node` in `memory-api/tools/mcp/session-mcp/src/server.rs`.

**AC4:** the `FinishBlocked` message in `memory-api/crates/session-api/src/store/config/persistence.rs` (`resolve_validation_gates`) now names both repair tools (`workflow_update_node`/`session_workflow_update_node`, `workflow_remove_node`/`session_workflow_remove_node`) instead of only stating the field is missing.

**AC7:** duplicate-`node_id` no-op behavior is now documented directly in the `session_workflow_add_node` / `session_workflow_add_nodes` tool `#[tool(description = ...)]` strings in `server.rs`, not only in the instruction file.

**AC6 test coverage** added to `memory-api/crates/session-api/src/store_tests/finish/validation_authority.rs`:
- `workflow_add_node_rejects_validation_kind_with_absent_spec_id`
- `workflow_add_nodes_rejects_unresolvable_validation_spec_id_with_index` (batch index + AC5 spec resolution)
- `workflow_add_node_rejects_ticket_and_spec_kinds_without_urn`
- `wedged_validation_node_is_repaired_via_update_node_and_finish_then_succeeds` (simulates a legacy wedge by writing `PersistedRuntimeContext` directly, bypassing create-time validation; asserts the `FinishBlocked` message names both repair tools; repairs via `workflow_update_node`; proves a finish round-trip succeeds after repair)
- `wedged_validation_node_is_repaired_via_remove_node` (same wedge simulation, repaired via `workflow_remove_node`)
- Fixed a pre-existing test (`workflow_finish_blocks_when_required_validation_guard_is_missing`) that previously created a `validation` node referencing a spec id that was never seeded in `.test` — now seeds the spec first so creation succeeds under the new AC5 check, and finish still blocks on the (unrelated, pre-existing) missing-execution path.

Spec [c677182e Durable session workflow graph and handoff continuity](memory-api/.spec/specs/c677182e-90da-4ac3-8b94-9e2e97c825cf/spec.toml) updated with a new "Node Creation Validation and Repair (ticket 980cf1fa)" section documenting the contract change.

**Validation:**
- `cargo test -p session-api` — 192 passed (10 suites)
- `cargo test -p session-mcp` — 12 passed (3 suites)

**Blocker/note:** `memory-api/tools/mcp/session-mcp/src/server.rs` is concurrently owned by another agent's board entry (ticket 459789f8, "workspace-validation parity tests"), which the draftboard already flags as `Conflict`. My server.rs edits (input structs, two new tool handlers, capability catalog entries, AC7 doc strings) compile cleanly and are validated by the green `cargo test -p session-mcp` run above, but board file-ownership could not be claimed for that file due to the pre-existing entry.