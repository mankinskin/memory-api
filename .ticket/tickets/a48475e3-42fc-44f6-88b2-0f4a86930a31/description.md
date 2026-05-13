# Plan: Structured Field Filtering on `ticket list`

## Problem

`ticket list` currently supports `--state` and `--type` filters only.
During the migration, agents needed to find tickets by:
- Title prefix: `Bug:`, `[bootstrap]`, `Plan:`, `Phase`
- Field value: `doc_category=bug-report`, `bug_validity=confirmed`
- Deleted status: `deleted=true`
- Missing field: tickets with `description.md` but no `doc_category`

Each of these required falling back to `grep` on raw TOML files (~10 times).

## Proposed Solution

Add a `--where` flag (or multiple `--field` filters) to `ticket list`:

```bash
# Filter by field value
ticket list --where 'doc_category=bug-report'

# Filter by title pattern
ticket list --where 'title starts_with "Bug:"'

# Filter by deleted status
ticket list --include-deleted

# Combine filters
ticket list --state open --where 'doc_category=plan'
```

### Implementation Notes
- Field filters are exact match by default
- Support `starts_with`, `contains`, `=`, `!=` operators
- `--include-deleted` flag to include soft-deleted tickets in results
- The redb index already stores field values — this should be efficient

## MCP Consideration

`list_tickets` MCP tool should accept optional field filters parameter.
