---
description: "Use when editing MCP tools (context-mcp, doc-viewer, log-viewer, ticket-mcp). Covers tool contracts, naming stability, and validation hooks."
applyTo: "tools/context-mcp/**,tools/doc-viewer/**,tools/log-viewer/**,tools/ticket-mcp/**"
---

# MCP Tools Guidance

## Contract Stability

- Treat tool names and schemas as compatibility boundaries.
- Do not rename or remove tools without a clear migration path.
- Keep tool descriptions aligned with current behavior.

## Tooling Workflow

Before changing MCP behavior:
1. Check existing docs for tool contracts.
2. Confirm whether behavior is already covered by tests.
3. Keep response formats stable unless explicitly requested.

After changing MCP behavior:
1. Run relevant tests.
2. Run documentation validation workflows.
3. Update related prompt/instruction text if tool behavior changed.

## Hooks and Validation

- Follow reminders from `.github/hooks/` after MCP-adjacent edits.
- Prefer doc-viewer validation flows over ad hoc manual checklists.
