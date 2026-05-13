# Phase 4: Copy Descriptions for Bootstrap Tickets

## Objective

Copy research phase docs as descriptions for the 13 bootstrap tickets that lack them. Set `doc_category=research`, `workflow_stage=plan`.

## Ticket Mapping

| Ticket (short) | Title | Research File |
|----------------|-------|---------------|
| `2a1fa2f2` | Lease lifecycle | `research/20260320_TASK_TRACKER_PLAN/015_phase_lease_protocol/PLAN.md` |
| `de6c3391` | Crash-recovery test | `research/20260320_TASK_TRACKER_PLAN/00_phase_contracts/PLAN.md` |
| `5e4727f9` | Deps, blocked-by, validate-graph | `research/20260320_TASK_TRACKER_PLAN/00_phase_contracts/PLAN.md` |
| `48ea4df8` | Dogfood trial | `research/20260320_TASK_TRACKER_PLAN/06_transition_dogfooding/PLAN.md` |
| `be1a3de7` | Merge queue scheduler | `research/20260320_TASK_TRACKER_PLAN/PROTOCOL_LAYER.md` |
| `c91a334e` | Observability standard | `research/20260320_TASK_TRACKER_PLAN/EXECUTION_CHECKLIST.md` |
| `a8d6c1d2` | [T1] Auth bootstrap | `research/20260320_TASK_TRACKER_PLAN/HOST_EXECUTOR_AUTH_PROVIDER.md` |
| `b1f3e2a4` | [T2] Branch/cwd checks | `research/20260320_TASK_TRACKER_PLAN/HOST_EXECUTOR_AUTH_PROVIDER.md` |
| `c2a4b6d8` | [T3] Ticket lifecycle | `research/20260320_TASK_TRACKER_PLAN/HOST_EXECUTOR_AUTH_PROVIDER.md` |
| `d3b5c7e9` | [T4] Validator handoff | `research/20260320_TASK_TRACKER_PLAN/VALIDATION_RELEASE_GOVERNANCE.md` |
| `e4c6d8f1` | [T5] Early-stop recovery | `research/20260320_TASK_TRACKER_PLAN/HOST_EXECUTOR_AUTH_PROVIDER.md` |
| `f5d7e9a2` | [T6] Merge/completion linkage | `research/20260320_TASK_TRACKER_PLAN/HOST_EXECUTOR_AUTH_PROVIDER.md` |

## Notes

- For T1-T6, the HOST_EXECUTOR_AUTH_PROVIDER.md contains sections for each ticket. Extract the relevant section or copy the full document and note which section applies.
- For shared phase docs (e.g., 00_phase_contracts used by both `de6c3391` and `5e4727f9`), copy the same file to both tickets' descriptions.

## Verification

- All 13 bootstrap tickets have `description.md`
- `ticket search "bootstrap"` returns all bootstrap tickets with content
