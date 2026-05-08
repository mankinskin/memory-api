<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=604a8966-cd45-4403-b3fb-90777e0f7d8a slug=shared/copilot-instructions/github-copilot-instructions/l1 -->
# GitHub Copilot Instructions

This file is intentionally minimal.

<!-- rule-api:entry id=53503cbf-4d68-42ef-91f8-fa35a3c3095b slug=shared/copilot-instructions/github-copilot-instructions/source-of-truth/l5 -->
## Source of Truth

All behavioral and workflow guidance lives in [AGENTS.md](../AGENTS.md).
Path-scoped guidance lives in [.github/instructions/](./instructions/).
Workflow prompts live in [.github/prompts/](./prompts/).

<!-- rule-api:entry id=a13bf9ab-3dd8-488d-8dc6-66f97e75aaf6 slug=shared/copilot-instructions/github-copilot-instructions/optional-copilot-cli-mcp-config/l11 -->
## Optional Copilot CLI MCP Config

To run Copilot CLI with MCP config from this repository:

<!-- rule-api:entry id=d9cce506-ef29-4217-abed-f3042ff48d6c slug=shared/copilot-instructions/github-copilot-instructions/optional-copilot-cli-mcp-config/l15 -->
```bash
copilot --additional-mcp-config @.github/copilot-mcp-config.json
```

<!-- rule-api:entry id=1c0066e5-ce52-49e0-b7ca-1685785882ac slug=shared/copilot-instructions/github-copilot-instructions/hooks/l19 -->
## Hooks

Hook reminders are configured in [.github/hooks/](./hooks/).
