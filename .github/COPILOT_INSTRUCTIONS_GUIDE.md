# GitHub Copilot Custom Instructions Guide

This repository uses GitHub Copilot's custom instructions feature to provide context to AI coding agents.

## Available Instruction Files

### 1. Repository-Wide Instructions
**File:** `AGENTS.md`
**Scope:** Project-wide behavioral guidance
**Usage:** Primary onboarding document for coding agents
**Status:** ✅ Active

### 2. Path-Specific Instructions
**Location:** `.github/instructions/*.instructions.md`
**Scope:** Files matching glob patterns specified in frontmatter
**Usage:** Crate-specific or feature-specific guidance
**Status:** ✅ Implemented

Example structure:
```
.github/instructions/
  ├── core-crates.instructions.md
  ├── ticket-system.instructions.md
  ├── mcp-tools.instructions.md
  ├── viewer-api-tools.instructions.md
  └── tests.instructions.md
```

### 3. Workflow Prompts
**Location:** `.github/prompts/*.prompt.md`
**Scope:** Explicitly invoked workflows
**Usage:** Task recipes (tickets, swarm-worker, debug-test, research)
**Status:** ✅ Implemented

### 4. Model-Specific Instructions (Alternative)
**Files:** `CLAUDE.md`, `GEMINI.md` (repository root only)
**Scope:** Specific AI model providers
**Usage:** Provider-specific optimizations
**Status:** Not used (provider-agnostic approach preferred)

## Current Setup

We use an **AGENTS-first + scoped instructions** approach:
- Primary file: `AGENTS.md`
- Path-scoped rules: `.github/instructions/*.instructions.md`
- Task prompts: `.github/prompts/*.prompt.md`
- Layered docs: `CHEAT_SHEET.md` → crate guides/README → tests/logs

## Future Enhancements

Consider adding path-specific instructions if:
- Crates have significantly different conventions
- Test files need specialized guidance
- Specific modules require detailed context

## References

- [GitHub Copilot Custom Instructions Docs](https://docs.github.com/en/copilot/customizing-copilot/adding-custom-instructions-for-github-copilot)
- [Custom Instructions Tutorial](https://docs.github.com/en/copilot/tutorials/customization-library/custom-instructions)
- [OpenAI agents.md repository](https://github.com/openai/agents.md)

## Validation

To verify Copilot is using your instructions:
1. Use Copilot Chat in VS Code or GitHub.com
2. Check the "References" section in responses
3. Look for `AGENTS.md` and matching `.github/instructions/*.instructions.md` in the reference list
4. Click to view the active instructions

## Enable/Disable

**In VS Code:** Settings → GitHub Copilot → Custom Instructions
**On GitHub.com:** Chat panel → Options (⋮) → "Enable/Disable custom instructions"
**For code review:** Repository Settings → Copilot → Code review → Toggle custom instructions
