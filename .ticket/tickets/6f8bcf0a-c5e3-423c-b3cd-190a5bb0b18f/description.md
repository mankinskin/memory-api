# Plan: Asset Management (`ticket attach`)

## Problem

Tickets can have an `assets/` directory for supplementary files (interview transcripts,
research documents, screenshots). Currently there's no CLI command — agents must
use raw `mkdir -p` + `cp` to add assets.

During the migration, ~7 asset copy operations were needed:
- 6 interview files attached to plan tickets
- 1 research file attached to a bug ticket

None of these were recorded in ticket history.

## Proposed Solution

```bash
# Attach a file (copies to assets/<filename>)
ticket attach <id> <file-path>

# Attach with custom name
ticket attach <id> <file-path> --as research_notes.md

# List assets
ticket assets <id>

# Remove asset
ticket detach <id> <asset-name>
```

### Behavior
- Copies file to `.ticket/tickets/<id>/assets/<name>`
- Creates `assets/` directory if needed
- Records attachment in `history.ndjson` as an event
- `ticket get` output includes asset list
- MCP `get_ticket` response includes asset metadata

## MCP Consideration

Add `attach_file` and `list_assets` MCP tools, or include asset info in `get_ticket`.
