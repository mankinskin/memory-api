# Plan: Bulk Field Updates (`ticket batch-update`)

## Problem

The existing `batch` command takes NDJSON `TaskCommand` objects, which is powerful
but requires building JSON payloads manually. During the migration, the common pattern
was: "set the same field on many tickets matching a condition."

This appeared ~8 times:
- Set `doc_category=bug-report` on all 8 bug tickets
- Set `doc_category=plan` on 27 tickets with descriptions
- Set `bug_validity=confirmed` on all 5 done bug tickets

Each required a Python script that loops over tickets and calls the CLI per-ticket.

## Proposed Solution

```bash
# Set field on all tickets matching a filter
ticket batch-update --where 'title starts_with "Bug:"' --field doc_category=bug-report

# Dry run first
ticket batch-update --where 'state=done' --field bug_validity=confirmed --dry-run

# Combine with state filter
ticket batch-update --state open --where 'doc_category=' --field doc_category=plan
```

### Behavior
- Applies `--field` updates to all tickets matching the filter
- Reports count of updated tickets
- `--dry-run` shows what would be updated without making changes
- Transactional: all-or-nothing (reuse existing batch infrastructure)

## Dependency

Depends on field filtering (ticket list --where) being implemented first.
