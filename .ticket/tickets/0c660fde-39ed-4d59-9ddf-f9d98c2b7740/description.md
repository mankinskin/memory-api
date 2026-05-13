# Plan: Deleted Ticket Visibility

## Problem

Soft-deleted tickets (`deleted = true`) are completely invisible to the CLI:
- `ticket list` excludes them
- `ticket get <id>` returns "not found"
- `ticket update <id>` returns "not found"

During the migration, 21 deleted tickets needed their state set to `cancelled`.
The only way to modify them was direct TOML file editing, bypassing the CLI entirely.

## Proposed Solution

```bash
# Include deleted in list
ticket list --include-deleted
ticket list --only-deleted

# Allow operations on deleted tickets
ticket get <id> --include-deleted
ticket update <id> --field state=cancelled --include-deleted

# Or: ticket undelete
ticket undelete <id>
```

### Behavior
- By default, deleted tickets remain hidden (no behavior change)
- `--include-deleted` flag makes them visible
- Operations on deleted tickets require explicit opt-in
- `ticket undelete` removes the `deleted` flag
