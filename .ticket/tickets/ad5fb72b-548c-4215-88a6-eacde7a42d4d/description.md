# Spec Health Check

## Objective

Validate spec store integrity including completeness, staleness, broken references, and coverage metrics.

## Health Checks

| Check | Description |
|-------|-------------|
| `missing_body` | Specs without body.md content |
| `missing_code_refs` | Implemented specs with no code references |
| `stale_refs` | Code refs pointing to moved/deleted files |
| `invalid_line_ranges` | Code refs with line ranges beyond file length |
| `orphan_children` | Specs referencing non-existent parent |
| `slug_conflicts` | Duplicate slugs (should be caught on write) |
| `stuck_state` | Specs in draft/reviewed for >30 days |
| `coverage_gap` | Public API symbols not covered by any spec |
| `dangling_edges` | Edges referencing deleted specs |

## Output

```json
{
  "total_specs": 42,
  "healthy": 38,
  "warnings": [
    { "spec_id": "...", "slug": "ticket-api/store", "check": "stale_refs", "message": "..." }
  ],
  "coverage": {
    "ticket-api": { "total_symbols": 120, "covered": 85, "percentage": 70.8 }
  }
}
```

## Acceptance Criteria

- [ ] All health checks implemented
- [ ] Coverage report per component
- [ ] JSON output for programmatic consumption
- [ ] CLI: `spec health --all` and `spec health <id|slug>`