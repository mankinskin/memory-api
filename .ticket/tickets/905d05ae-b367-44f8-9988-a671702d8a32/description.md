## Problem

`test-mcp`'s write tools (`test_record_spec`, `test_record_execution`) resolve their `.test` store root per-call from an explicit `workspace` argument via `config_for_workspace` (walks upward from the given path to find/create a nested `.test` store). The read tools (`test_get_spec`, `test_get_execution`, `test_list_specs`, `test_list_executions`) have no `workspace` parameter at all and always read from the server process's fixed `store_root` captured at startup (`self.config()`).

Consequence: recording evidence with `workspace=<memory-api path>` (which resolves to the nested `memory-api/.test` store) makes that evidence **invisible** to `test_list_executions`/`test_list_specs` when the server was started with its fixed root at the repo root (`context-engine/.test`), and vice versa. Confirmed on disk: `context-engine/.test/default/executions` has 15 execution files, `memory-api/.test/default/executions` has 17, with zero overlap. Evidence recorded for the feedback-api ring-activation tickets (PROV/INGEST/MAP/BFS/SYNTH) went to the nested store and was undiscoverable via `test_list_executions` at the root.

## Fix

Add an optional `workspace` parameter to the four read tools (`test_get_spec`, `test_get_execution`, `test_list_specs`, `test_list_executions`), mirroring the write tools' `config_for_workspace` resolution when explicitly supplied. When `workspace` is omitted (the common/default case), aggregate reads across every `.test` store discoverable from the server's workspace root via the existing `memory_api::workspace::discover_workspace_store_roots` helper (the same policy-aware descendant-discovery mechanism `ticket-api`/`spec-api`/`rule-api` already use for their own `default` workspace aggregation), instead of only reading the single fixed root. Merge, dedupe by id, and re-apply sort/limit globally across the aggregated result set for `list_executions`.

## Acceptance Criteria

- `test_list_executions` (no `workspace` arg) returns executions recorded to both `context-engine/.test` and `memory-api/.test` (and any other descendant `.test` store under the active workspace policy), not just the server's fixed root.
- `test_list_specs`, `test_get_spec`, `test_get_execution` behave consistently (aggregate on omitted `workspace`; resolve to one explicit store when `workspace` is supplied).
- Existing single-store callers (explicit `workspace` argument) are unaffected.
- `list_executions`/`list_specs` still respect `outcome`/`ticket_id`/`sort`/`limit` filters correctly after aggregation.
- Unit tests cover: aggregation finds executions/specs from a nested descendant store; explicit `workspace` param still pins to one store; sort+limit correctness after merging results from multiple stores.

## Evidence

Discovered while reviewing tickets a7601cb7 (PROV), b4954d6c (INGEST), 16e112a7 (MAP), 3fa60398 (BFS), 3d4c4739 (SYNTH) for the feedback-api ring activation: `test_list_executions` (root store) could not find BFS/SYNTH executions recorded to the nested `memory-api/.test` store, and PROV/INGEST/MAP had no linked executions in either store.