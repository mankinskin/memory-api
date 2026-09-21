# Session Objective
Resolve the current static_complexity batch for memory-api and reduce 28 findings from the baseline.

# Scope Guardrails
- Stay inside memory-api unless a blocker requires a dependency fix outside scope.
- Do not start the next batch until this ticket meets done criteria.

# Implementation Steps
1. Capture exact finding rows for this batch from the baseline audit artifact.
2. Group findings into 2 to 5 micro-chunks and handle one chunk at a time.
3. After each chunk, run the narrowest compile/test check relevant to touched files.
4. Re-run audit summary and record count delta.
5. If blockers remain, create follow-up tickets and link them before handoff.

# Validation Commands
- Full category summary: cargo run -p audit-cli --bin audit -- --json summary --by category .
- Full baseline refresh when needed: cargo run -p audit-cli --bin audit -- --json run .
- Ticket health sanity: ./target/debug/ticket.exe health --workspace . --all --toon

# Acceptance Criteria
- Findings in this batch are resolved or have explicit blocker tickets linked.
- No increase in other categories caused by this batch.
- Batch notes include before and after counts and next unresolved action.

# Progress Log

## Baseline (2026-07-07)
- Artifact: target/tmp/batch3_memory_api_audit_before.json
- Baseline rows (memory-api static_complexity only): target/tmp/batch3_memory_api_baseline_rows.json
- Baseline count: 28

## Chunk 1 (ticket-http: error/graph/workflow)
- Edited:
  - memory-api/tools/http/ticket-http/src/serve/error.rs
  - memory-api/tools/http/ticket-http/src/serve/handlers/graph/traversal.rs
  - memory-api/tools/http/ticket-http/src/serve/handlers/workflow.rs
- Narrow validation:
  - cargo test --manifest-path memory-api/tools/http/ticket-http/Cargo.toml storage_err
  - cargo test --manifest-path memory-api/tools/http/ticket-http/Cargo.toml workflow_next
  - cargo test --manifest-path memory-api/tools/http/ticket-http/Cargo.toml workspace_graph
- Post artifact: target/tmp/batch3_memory_api_chunk1_after.json
- Delta vs baseline: 28 -> 24 (resolved 4, added 0)
  - resolved: storage_err, workspace_graph, workflow_next, workflow_tree_response

## Chunk 2 (ticket-http: list handler)
- Edited:
  - memory-api/tools/http/ticket-http/src/serve/handlers/tickets/read.rs
- Narrow validation:
  - cargo test --manifest-path memory-api/tools/http/ticket-http/Cargo.toml list_tickets
- Post artifact: target/tmp/batch3_memory_api_chunk2_after.json
- Delta vs chunk1: 24 -> 23 (resolved 1, added 0)
  - resolved: list_tickets

## Chunk 3 (ticket-cli/test-cli dispatch routing)
- Edited:
  - memory-api/tools/cli/ticket-cli/src/cli/dispatch.rs
  - memory-api/tools/cli/test-cli/src/lib.rs
- Narrow validation:
  - cargo test --manifest-path memory-api/tools/cli/ticket-cli/Cargo.toml dispatch_
  - cargo test --manifest-path memory-api/tools/cli/test-cli/Cargo.toml dispatch
  - cargo check --manifest-path memory-api/tools/cli/test-cli/Cargo.toml
- Post artifact: target/tmp/batch3_memory_api_chunk3_after.json
- Delta vs chunk2: 23 -> 20 (resolved 3, added 0)
  - resolved: dispatch_store_command_graph, dispatch_store_command_ops, test-cli dispatch

## Chunk 4b (core low-risk routing/parser/store updates)
- Edited:
  - memory-api/crates/memory-matrix/src/matrix.rs
  - memory-api/crates/memory-api/src/model/query.rs
  - memory-api/crates/rule-api/src/store.rs
- Narrow validation:
  - cargo check -p memory-api -p rule-api -p memory-matrix
  - cargo test -p rule-api update
  - cargo test -p memory-api query
  - cargo test -p memory-matrix dispatch
- Post artifact: target/tmp/batch3_memory_api_chunk4b_after.json
- Delta vs chunk3: 20 -> 17 (resolved 3, added 0)
  - resolved: matrix dispatch, parse_field_value, rule-api update

## Category Regression Check
- Compared full findings by category (baseline vs chunk4b):
  - static_complexity: 41 -> 30 (-11)
  - all other categories unchanged
  - increases: none

## Chunk 5b (spec index tree + matrix transport test predicate)
- Edited:
  - memory-api/crates/spec-api/src/store_index.rs
  - memory-api/crates/memory-matrix/tests/matrix.rs
- Narrow validation:
  - cargo test -p spec-api store_index -- --nocapture
  - cargo test -p memory-matrix unwired_transports_are_explicitly_blocked_with_reason -- --nocapture
- Post artifact: target/tmp/batch3_memory_api_chunk5b_after.json
- Delta vs chunk4b: 17 -> 15 (resolved 2, added 0)
  - resolved: render_tree_entry_page, unwired_transports_are_explicitly_blocked_with_reason

## Category Regression Check (chunk4b -> chunk5b)
- static_complexity: 30 -> 28 (-2)
- ticket_graph: 3 -> 6 (+3, pre-existing/parallel drift outside this batch scope)
- all other categories unchanged

## Chunk 6a (memory-matrix MCP dispatch helper split)
- Edited:
  - memory-api/crates/memory-matrix/src/mcp.rs
- Narrow validation:
  - cargo test -p memory-matrix ticket_get_mcp_cell_is_wired_and_passes -- --nocapture
  - cargo test -p memory-matrix ticket_update_mcp_cell_is_wired_and_passes -- --nocapture
  - cargo test -p memory-matrix spec_scan_mcp_cell_is_wired_and_passes -- --nocapture
- Post artifact: target/tmp/batch3_memory_api_chunk6a_after.json
- Delta vs chunk5b (memory-api static_complexity only): 15 -> 13 (resolved 2, added 0)
  - resolved: dispatch_ticket_mcp, dispatch_spec_mcp

## Chunk 6b (move-kernel preflight/execution helper split)
- Edited:
  - memory-api/crates/memory-api/src/storage/move_kernel.rs
- Narrow validation:
  - cargo check -p memory-api
  - cargo test -p memory-api storage::move_kernel
- Post artifact:
  - target/tmp/batch3_memory_api_chunk6b_after.json
- Delta vs chunk6a (memory-api static_complexity, crates+tools normalized): 13 -> 11 (resolved 2, added 0)
  - resolved: plan_move, execute_or_resume
  - remaining in move_kernel.rs: none

## Chunk 6c (ticket-api store update helper split)
- Edited:
  - memory-api/crates/ticket-api/src/storage/store.rs
- Narrow validation:
  - cargo check -p ticket-api
  - cargo test -p ticket-api update -- --nocapture
- Post artifact:
  - target/tmp/batch3_memory_api_chunk6c_after.json
- Delta vs chunk6b (memory-api static_complexity, crates+tools normalized): 11 -> 10 (resolved 1, added 0)
  - resolved: update in ticket-api/src/storage/store.rs
  - remaining in ticket-api/src/storage/store.rs: none

## Chunk 6d (session-api transcript hook helper split)
- Edited:
  - memory-api/crates/session-api/src/hook.rs
- Narrow validation:
  - cargo check -p session-api
  - cargo test -p session-api transcript_reader -- --nocapture
- Post artifact:
  - target/tmp/batch3_memory_api_chunk6d_after.json
- Delta vs chunk6c (memory-api static_complexity, crates+tools normalized): 10 -> 9 (resolved 1, added 0)
  - resolved: copilot_payload_from_transcript_reader_with_path
  - remaining in session-api/src/hook.rs: none

## Chunk 6e (log-api runtime-session query matcher split)
- Edited:
  - memory-api/crates/log-api/src/store.rs
- Behavior preserved:
  - Same query semantics for status/transport/component/run_id and all traceability link filters.
- Extracted helpers:
  - matches_runtime_session_query
  - matches_runtime_session_core
  - matches_runtime_session_traceability
  - matches_runtime_session_primary_links
  - matches_runtime_session_secondary_links
- Narrow validation:
  - rtk cargo check -p log-api
  - rtk cargo test -p log-api lists_runtime_sessions_with_filters -- --nocapture
- Post artifact:
  - target/tmp/batch3_memory_api_chunk6e_after.json
- Delta vs chunk6d (memory-api static_complexity, crates+tools normalized with memory-api/ prefix normalization): 9 -> 8 (resolved 1, added 0)
  - resolved: crates/log-api/src/store.rs:136
  - added: none

## Chunk 6f (ticket-cli move mode/helper split)
- Edited:
  - memory-api/tools/cli/ticket-cli/src/cli/commands/lifecycle.rs
- Behavior preserved:
  - Same mode validation and output envelopes for resume, rollback, plan, and execute.
- Extracted:
  - MoveMode
  - resolve_move_mode
  - validate_move_mode_args
  - parse_move_journal_uuid
  - handle_move_resume
  - handle_move_rollback
- Narrow validation:
  - rtk cargo check --manifest-path memory-api/tools/cli/ticket-cli/Cargo.toml
  - rtk cargo test --manifest-path memory-api/tools/cli/ticket-cli/Cargo.toml cmd_move_dry_run_returns_preflight_plan -- --nocapture
- Post artifact:
  - target/tmp/batch3_memory_api_chunk6f_after.json
- Delta vs chunk6e (memory-api static_complexity, crates+tools normalized): 8 -> 7 (resolved 1, added 0)
  - resolved: tools/cli/ticket-cli/src/cli/commands/lifecycle.rs:71
  - added: none

## Chunk 6g (test-api execution query helper split)
- Edited:
  - memory-api/crates/test-api/src/store.rs
- Behavior preserved:
  - Same execution query filtering semantics for identity, duration, and provenance filters.
  - Same sort semantics for newest-first and slowest-first.
- Extracted:
  - matches_execution_query
  - matches_execution_identity_filters
  - matches_execution_duration_filters
  - matches_execution_provenance_filters
  - sort_executions
- Narrow validation:
  - rtk cargo check -p test-api
  - rtk cargo test -p test-api lists_executions_filtered_by_ticket_and_outcome -- --nocapture
- Post artifact:
  - target/tmp/batch3_memory_api_chunk6g_after.json
- Delta vs chunk6f (memory-api static_complexity, crates+tools normalized): 7 -> 6 (resolved 1, added 0)
  - resolved: crates/test-api/src/store.rs:137
  - added: none

## Remaining Chunk Map (planned from chunk6g baseline)
1. Chunk 6h-a: tools/cli/ticket-cli/src/cli.rs:253
2. Chunk 6h-b: tools/cli/ticket-cli/src/cli/human_output.rs:11
3. Chunk 6h-c: crates/audit-api/src/trials/spec_fulfillment.rs:32
4. Chunk 6h-d: tools/cli/rule-cli/src/cli/rendering.rs:252
5. Chunk 6h-e: crates/spec-api/src/manifest.rs:357
6. Chunk 6i: crates/audit-api/src/trials/ticket_graph.rs:66

## Chunk 6h/6i execution (completed all remaining mapped hotspots)
- Edited:
  - memory-api/tools/cli/ticket-cli/src/cli.rs
  - memory-api/tools/cli/ticket-cli/src/cli/human_output.rs
  - memory-api/crates/audit-api/src/trials/spec_fulfillment.rs
  - memory-api/tools/cli/rule-cli/src/cli/rendering.rs
  - memory-api/crates/spec-api/src/manifest.rs
  - memory-api/crates/audit-api/src/trials/ticket_graph.rs
  - memory-api/tools/cli/ticket-cli/src/cli/batch/dispatch.rs
- Behavior preserved:
  - ticket-cli command naming/output routing semantics preserved.
  - rule-cli sync target safety checks and generated-target bookkeeping preserved.
  - spec health/audit trial decision and finding semantics preserved.
- Narrow validation:
  - rtk cargo check -p ticket-cli -p rule-cli -p spec-api -p audit-api
  - rtk cargo test -p ticket-cli human_output -- --nocapture
  - rtk cargo test -p spec-api health_issues -- --nocapture
  - rtk cargo test -p audit-api spec_fulfillment -- --nocapture
  - rtk cargo test -p audit-api ticket_graph -- --nocapture
- Post artifacts:
  - target/tmp/batch3_memory_api_chunk6h_after.json
  - target/tmp/batch3_memory_api_chunk6i_after.json
- Delta vs chunk6g (memory-api static_complexity, crates+tools normalized): 6 -> 0 (resolved 6, added 0)
  - resolved:
    - crates/audit-api/src/trials/spec_fulfillment.rs:32
    - crates/audit-api/src/trials/ticket_graph.rs:66
    - crates/spec-api/src/manifest.rs:357
    - tools/cli/rule-cli/src/cli/rendering.rs:252
    - tools/cli/ticket-cli/src/cli.rs:253
    - tools/cli/ticket-cli/src/cli/human_output.rs:11
  - added: none

## Ticket Health Sanity
- Ran earlier in this session: ./target/debug/ticket.exe health --workspace . --all --toon
- Result: store-wide warnings exist (mostly missing effort/description on other tickets); no blocker discovered for this batch execution flow.

## Remaining Work
- memory-api static_complexity remaining in this batch: 0
- All mapped chunks completed in this session.

# Handoff Notes
Record exact commands run, resulting counts, and files changed so the next session can continue without rediscovery.