# Feature Tracking

## Objective

Track per-spec feature completeness: which features are implemented, planned, blocked, or have known bugs.

## Data Model

Stored in spec.toml:

```toml
[features]
implemented = ["create", "get", "update", "delete"]
planned = ["bulk_update", "import_export"]
blocked = ["streaming_read"]
bugs = ["stale index after concurrent writes"]
```

## Operations

- `spec feature add <spec> --implemented <feature>`
- `spec feature add <spec> --planned <feature>`
- `spec feature add <spec> --bug <description>`
- `spec feature move <spec> <feature> --from planned --to implemented`
- `spec feature list <spec>` — show feature breakdown
- `spec feature summary [--component <name>]` — aggregate stats

## Acceptance Criteria

- [ ] Feature status stored in spec.toml under `[features]`
- [ ] CLI commands for managing feature status
- [ ] Summary aggregation across component specs
- [ ] Integration with skill generation (bugs → pitfalls section)