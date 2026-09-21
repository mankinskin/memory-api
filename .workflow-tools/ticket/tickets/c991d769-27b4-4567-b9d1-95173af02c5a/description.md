# Session Objective
Resolve the current file_length batch for memory-api and reduce findings from baseline using strict largest-first splits.

# Planning Update (2026-07-08)
The remaining batch-1 work is now grouped into focused child tickets by subsystem to reduce context switching and keep validation scoped.

Current remaining findings in this batch scope: **88**.

## Grouped execution tickets
1. batch-1b rules+fixtures (largest in queue now includes `memory-fixtures/src/lib.rs` and `rule-api/targets.rs`)
2. batch-1c ticket surfaces (ticket-http/ticket-cli/ticket-mcp oversized files)
3. batch-1a core crates (memory-api/ticket-api/spec-api/session-api/test-api/audit-api)
4. batch-1d tools+matrix tail (matrix + spec-cli/rule-cli tail files)

## Execution policy
- Keep strict largest-first selection globally from fresh audit ranking.
- Apply behavior-preserving splits only (helper/test module extraction first).
- Run focused crate/tool validation after each split and checkpoint commits.
- Keep this parent ticket as tracker; close only after all child group tickets are done.