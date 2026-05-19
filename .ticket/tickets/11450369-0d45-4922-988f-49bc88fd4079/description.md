Render `ticket board show` text output as a dashboard-only summary instead of a dashboard plus a raw structured dump.

Acceptance criteria:
- default text output stops after the board-specific human renderer
- `Next Up` renders compact pretty cards while preserving all recommendation keys
- human `created_at` uses a compact pretty timestamp
- JSON output remains unchanged