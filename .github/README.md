# GitHub Copilot Configuration

This directory contains GitHub Copilot custom instructions for the context-engine project.

## Active Files

### Primary Agent Instructions
**`../AGENTS.md`** - Repository-wide behavioral constitution

This file provides:
- operating principles and scope control
- MCP-first discovery protocol
- task routing for simple fixes, bugs, features, research, and swarm work
- quality gates, escalation rules, and fallback behavior

### Supporting Documentation
**`COPILOT_INSTRUCTIONS_GUIDE.md`** - Meta-documentation about the custom instructions system
**`.github/instructions/*.instructions.md`** - Path-scoped guidance loaded by file pattern
Includes `viewer-api-tools.instructions.md` for viewer-api-driven frontend and API/file integration work.
**`.github/prompts/*.prompt.md`** - Explicit workflow prompts

## GitHub Copilot Instruction Types

GitHub Copilot supports multiple instruction file types (in priority order):

1. **Agent instructions**: `AGENTS.md` ✅ **Primary in this repo**
   - Applied broadly for project-level conventions
   - Used as the single source of core behavior

2. **Path-specific**: `.github/instructions/*.instructions.md`
   - Target specific files/directories with glob patterns
   - Use for crate/tool/test specific guidance
   - Implemented in this repository

3. **Prompt workflows**: `.github/prompts/*.prompt.md`
   - Named workflows invoked explicitly (for example, tickets, swarm-worker, debug-test, research)

4. **Model-specific**: `CLAUDE.md` or `GEMINI.md` (root only)
   - Provider-specific optimizations
   - Not used (provider-agnostic approach preferred)

## Documentation Architecture

The custom instructions reference a layered documentation structure:

```
Priority Order (for agents):
1. CHEAT_SHEET.md                    ← Quick reference (types, patterns, gotchas)
2. <crate>/HIGH_LEVEL_GUIDE.md       ← Conceptual understanding
3. <crate>/README.md                 ← API overview
4. <crate>/DOCUMENTATION_ANALYSIS.md ← Detailed structural analysis
5. <crate>/src/tests/                ← Usage examples
6. bug-reports/                      ← Known issues
7. QUESTIONS_FOR_AUTHOR.md           ← Unclear topics
8. .github/prompts/*.prompt.md       ← Operational workflows
```

## Verification

To verify Copilot is using these instructions:

**In VS Code:**
- Use Copilot Chat
- Check "References" in response
- Look for `AGENTS.md` and/or matching `.github/instructions/*.instructions.md`

**On GitHub.com:**
- Use Copilot Chat or Agents
- Expand "References" at top of response
- Verify custom instructions file is listed

**For Code Review:**
- Repository Settings → Copilot → Code review
- Verify "Use custom instructions" is enabled

## Updates

When updating instruction architecture:
- keep `AGENTS.md` concise and stable
- place domain rules in `.github/instructions/*.instructions.md`
- place task recipes in `.github/prompts/*.prompt.md`
- avoid duplicated copies of the same instruction content

## References

- [GitHub Copilot Custom Instructions Docs](https://docs.github.com/en/copilot/customizing-copilot/adding-custom-instructions-for-github-copilot)
- [Custom Instructions Examples](https://docs.github.com/en/copilot/tutorials/customization-library/custom-instructions)
- [OpenAI agents.md](https://github.com/openai/agents.md)
