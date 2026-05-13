# Phase 3: Attach Interviews as Assets

## Objective

Copy interview files into `assets/interviews/` for their parent plan tickets. Interviews are supplementary to plans and should not be standalone tickets.

## Files to Migrate

| Interview File | Parent Ticket | Ticket Title |
|----------------|---------------|--------------|
| `interviews/20260310_INTERVIEW_CONTEXT_API.md` | `0727b7dd` | Context API master plan |
| `interviews/20260303_VIEWER_REFACTORING_AND_MOBILE.md` | `20c4d807` | Viewer refactoring + mobile |
| `interviews/20260303_SEARCH_EVENT_REFACTORING.md` | `d265e603` | Search event refactoring |
| `interviews/20260301_VIEWER_TOOLS_FEATURE_PLAN.md` | `68912b00` | Viewer tools features |
| `interviews/20260307_INTERVIEW_NESTING_VIEW.md` | `ee7aa0cd` | Nesting view mode |
| `interviews/20260315_INTERVIEW_CONTEXT_READ_RESTRUCTURE.md` | `668743ea` | Context-read restructure |

## Steps

For each interview:
1. `mkdir -p .ticket/tickets/<uuid>/assets/interviews/`
2. `cp agents/<interview-file> .ticket/tickets/<uuid>/assets/interviews/`
3. Run `ticket scan --reindex` to pick up changes

## Verification

- Each ticket's `assets/interviews/` directory contains its interview file
- `ls .ticket/tickets/<uuid>/assets/interviews/` returns the expected file
