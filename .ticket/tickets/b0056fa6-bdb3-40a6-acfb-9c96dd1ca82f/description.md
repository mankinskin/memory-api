# Plan: Ticket Store Audit (`ticket audit`)

## Problem

After the migration, a 90-line Python script was needed to audit the ticket store:
- Count tickets by state and category
- Find tickets with `description.md` but no `doc_category`
- Detect "limbo" tickets (open, no description, no planned work)
- Validate `doc_category` values against allowed set
- Check for deleted-but-not-cancelled inconsistencies

This kind of health check is useful for ongoing maintenance, not just one-time migrations.

## Proposed Solution

```bash
# Full audit report
ticket audit

# Specific checks
ticket audit --check missing-fields
ticket audit --check limbo
ticket audit --check category-distribution

# Stats only
ticket stats
ticket stats --by doc_category
ticket stats --by state
ticket stats --by component
```

### Audit Checks
1. **missing-fields**: Tickets with description but no `doc_category`
2. **limbo**: Open tickets with no description and no assignee
3. **category-validity**: `doc_category` values not in allowed set
4. **deleted-state**: Deleted tickets not in `cancelled` state
5. **orphan-assets**: Assets in ticket dirs not referenced anywhere
6. **stale-leases**: Leases that expired but weren't released

### Output
- Summary table with counts
- Issue list with severity (error/warning/info)
- Exit code 1 if errors found (CI-friendly)
