# Plan: Migrate Agent Files into Ticket System

## Objective

Migrate the 201 agent documentation files from `agents/` into the ticket system as structured, filterable ticket content. Add schema fields so all tickets can be queried by category, tags, workflow stage, and provenance.

## Context

### Current State
- **106 total tickets** (59 open, 29 done, 12 in-progress, 6 review)
- **53 tickets** already have `description.md`
- **53 tickets** are missing `description.md`
- **201 agent files** across 9 categories: plans (60), research (39), implemented (27), analysis (20), guides (19), tmp (18), bug-reports (7), interviews (6), designs (2)
- **44 open tickets** cross-referenced to agent files — 42 have matching files
- **30+ ad-hoc fields** in use beyond the 14 defined schema fields

### Problem
Agent files contain the authoritative plans, designs, bug analyses, and research for tickets — but they live outside the ticket system. This means:
- `ticket search` can't find plan content
- `ticket list --field` can't filter by document category or tags
- Interviews, research, and analysis aren't linked to their parent tickets
- 30+ ad-hoc fields create inconsistency in filtering

### Cross-Reference Map
See session memory `/memories/session/ticket-agent-file-map.md` for the full 44-ticket mapping.

## Execution Phases

### Phase 1: Schema Improvements
Add 7 new fields to `crates/ticket-api/schemas/tracker-improvement.toml`:

| Field | Type | Values | Purpose |
|-------|------|--------|---------|
| `doc_category` | string | `plan\|bug-report\|design\|interview\|research\|guide\|analysis\|impl-record` | Document type classification |
| `tags` | string | JSON array | Frontmatter tags for search (e.g. `["#context-api","#architecture"]`) |
| `workflow_stage` | string | `design\|plan\|implementation\|validation\|done` | Formalize existing ad-hoc field (31 tickets) |
| `priority` | string | `low\|medium\|high\|critical` | Urgency axis (separate from `risk_level`) |
| `source_agent_files` | string | JSON array | Original agent file paths for provenance |
| `bug_validity` | string | `not_confirmed\|confirmed` | Bug validity status — `confirmed` when latest reproduction succeeded |
| `phase` | string | Free-form | Multi-phase plan identifier (e.g. `phase-3.1`) |

**Acceptance criteria:** Schema TOML updated, `ticket scan --reindex` succeeds, existing tickets unaffected.

### Phase 2: Copy Descriptions for Open Plan Tickets (18 tickets)
For each open ticket that has a matching agent plan file but no `description.md`:

1. Copy primary plan file as `description.md`
2. Set `doc_category=plan`, `workflow_stage=plan`
3. Extract tags from YAML frontmatter → `tags` field
4. Set `source_agent_files` to original path

**Tickets:**
- `619e49fc` ← `plans/20260115_PLAN_fine_grained_locking.md`
- `81a6a595` ← `plans/20260314_PLAN_CONTEXT_API_PHASE4_1.md`
- Plus ~16 others from the cross-reference map

**Acceptance criteria:** All 18 tickets have `description.md`; `ticket search` finds plan content.

### Phase 3: Attach Interviews as Assets (6 files → 6 tickets)
Copy interview files into `assets/interviews/` for their parent plan tickets:

| Interview File | Parent Ticket |
|----------------|---------------|
| `interviews/20260310_INTERVIEW_CONTEXT_API.md` | `0727b7dd` |
| `interviews/20260303_VIEWER_REFACTORING_AND_MOBILE.md` | `20c4d807` |
| `interviews/20260303_SEARCH_EVENT_REFACTORING.md` | `d265e603` |
| `interviews/20260301_VIEWER_TOOLS_FEATURE_PLAN.md` | `68912b00` |
| `interviews/20260307_INTERVIEW_NESTING_VIEW.md` | `ee7aa0cd` |
| `interviews/20260315_INTERVIEW_CONTEXT_READ_RESTRUCTURE.md` | `668743ea` |

**Acceptance criteria:** Each ticket's `assets/interviews/` directory contains its interview file.

### Phase 4: Copy Descriptions for Bootstrap Tickets (13 tickets)
Copy research phase docs as descriptions for bootstrap tickets lacking them:

| Ticket | Research File |
|--------|---------------|
| `2a1fa2f2` (lease lifecycle) | `research/.../015_phase_lease_protocol/PLAN.md` |
| `de6c3391` (crash-recovery) | `research/.../00_phase_contracts/PLAN.md` |
| `5e4727f9` (deps, validate-graph) | `research/.../00_phase_contracts/PLAN.md` |
| `48ea4df8` (dogfood trial) | `research/.../06_transition_dogfooding/PLAN.md` |
| `a8d6c1d2`–`f5d7e9a2` (T1–T6) | Relevant sections from `HOST_EXECUTOR_AUTH_PROVIDER.md` |
| Plus `be1a3de7`, `c91a334e` | From `PROTOCOL_LAYER.md`, `EXECUTION_CHECKLIST.md` |

Set `doc_category=research`, `workflow_stage=plan`.

**Acceptance criteria:** All 13 bootstrap tickets have `description.md`.

### Phase 5: Bug Ticket Enrichment (bug validity + reproduction tracking)
- Set `bug_validity` field on all bug tickets: `not_confirmed` (default) or `confirmed` (latest reproduction succeeded)
- Record reproduction evidence via `ticket repro` for bugs with agent bug-report files containing reproduction data
- Attach research files as `assets/research/` where applicable
- Set `doc_category=bug-report` on all bug tickets
- Reproduction status is derived from existing `reproductions` array stored by `ticket repro`:
  - No reproduction tested → array empty/missing
  - Reproduction failed → latest outcome ≠ `reproduced` (with commit + timestamp)
  - Reproduction succeeded → latest outcome = `reproduced` (with commit + timestamp)

**Acceptance criteria:** Bug tickets have `doc_category=bug-report`, `bug_validity` set, and reproduction evidence recorded via `ticket repro`.

### Phase 6: Cleanup and Deduplication
1. **21 `deleted=true` tickets** → transition to `cancelled` state, remove `deleted` field
2. **Duplicate tickets** → identify and merge (link as `linked`, cancel duplicate):
   - `7452808b` duplicates `346573c1` (cache root mismatch)
   - `c863dc82` duplicates `b786f1f5` (CLI read UX)
   - `c2d81058` duplicates `0727b7dd` (Context API)
   - `cf3f8a76` duplicates `974e6e37` (Instruction Language DSL)
   - `aa46add2` duplicates `346573c1` (cache root mismatch)
   - `7d5a115a` duplicates `3125d4c5` (context-read errors)
   - `ad29a401` duplicates `9f4cf3ad` (Root update steps)
   - `ecb1518c` duplicates `7bf50e75` (Sandboxed integration tests)
   - `c320a858` duplicates `978ce8a5` (Expansion loop redesign)
3. **Bug `3125d4c5`** — not reproduced, consider closing
4. **Set `doc_category`** on all remaining tickets that already have descriptions

**Acceptance criteria:** No `deleted` field in use; duplicate tickets resolved; all tickets with descriptions have `doc_category` set.

## Out of Scope (Keep as Repo Docs)
- `agents/guides/` (19 files) — reference documentation, not work items
- `agents/analysis/` (20 files) — historical analysis, not actionable
- `agents/implemented/` (27 files) — completion records for past work
- `agents/tmp/` (18 files) — scratch work

## Risks
- **Medium:** Schema changes could affect existing ticket queries if field names collide with ad-hoc fields already in use (mitigated: `workflow_stage` is the only collision, and formalizing it is the goal)
- **Low:** Large batch of file copies could introduce errors (mitigated: verify with `ticket get` after each batch)

## Validation
After all phases:
```bash
ticket list --field doc_category=plan     # Should return all plan tickets
ticket list --field doc_category=bug-report  # Should return all bug tickets
ticket search "#context-api"              # Should find tagged tickets
```
