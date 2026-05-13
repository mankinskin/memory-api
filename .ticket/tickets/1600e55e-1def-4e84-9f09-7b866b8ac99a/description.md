# Phase 2: Copy Descriptions for Open Plan Tickets

## Objective

For each open ticket that has a matching agent plan file but no `description.md`, copy the primary plan file as the ticket's description and set structured metadata fields.

## Scope

~18 open plan tickets currently missing description bodies. For each:

1. Copy primary plan file as `.ticket/tickets/<uuid>/description.md`
2. Set `doc_category=plan`, `workflow_stage=plan`
3. Extract tags from YAML frontmatter → `tags` field
4. Set `source_agent_files` to original agent file path

## Ticket List

| Ticket (short) | Title | Agent File |
|----------------|-------|------------|
| `619e49fc` | Fine-grained locking | `plans/20260115_PLAN_fine_grained_locking.md` |
| `81a6a595` | Context API phase 4.1 | `plans/20260314_PLAN_CONTEXT_API_PHASE4_1.md` |
| Plus ~16 others from the cross-reference map in the parent ticket |

## Verification

- `ticket get --id <uuid>` shows `doc_category=plan`
- `ticket search "<keyword from plan>"` returns the ticket
- All 18 tickets have non-empty `description.md`

## Notes

For tickets that already have descriptions, verify they match the agent file content. Only add missing fields (`doc_category`, `tags`, `source_agent_files`).
