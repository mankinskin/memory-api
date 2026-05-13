# Spec-to-Code Sync

## Objective

Detect when implementation code changes and update spec code references and feature status accordingly.

## Sync Operations

1. **Reference update**: When a file's line numbers shift, update `line_start`/`line_end` in affected code_refs
2. **Symbol rename**: When a symbol is renamed, update the `symbol` field in code_refs
3. **Feature status**: Mark features as `implemented` when corresponding code appears, `blocked` when code is removed
4. **Staleness detection**: Compare file mtimes or content hashes against last spec update

## CLI Interface

```
spec sync [<id|slug>] [--all] [--dry-run]
spec sync --check  # report stale specs without updating
```

## Acceptance Criteria

- [ ] `spec sync` updates line numbers in code refs
- [ ] Staleness detection via file hash comparison
- [ ] Dry-run mode shows proposed changes
- [ ] `spec sync --check` reports without modifying