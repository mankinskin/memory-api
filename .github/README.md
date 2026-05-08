<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=22de51c6-027b-40ad-8a6d-6b1a7f5157cd slug=shared/github-readme/github-copilot-configuration/l1 -->
# GitHub Copilot Configuration

This directory contains GitHub Copilot custom instructions for the context-engine project.

<!-- rule-api:entry id=eac0b830-3739-47db-8cb8-3d589821f1a8 slug=shared/github-readme/github-copilot-configuration/active-files/primary-agent-instructions/l5 -->
## Active Files

### Primary Agent Instructions
**`../AGENTS.md`** - Repository-wide behavioral constitution

<!-- rule-api:entry id=cf892cce-4119-418c-8d25-45fec7f514f0 slug=shared/github-readme/github-copilot-configuration/active-files/primary-agent-instructions/l10 -->
This file provides:
- operating principles and scope control
- MCP-first discovery protocol
- task routing for simple fixes, bugs, features, research, and swarm work
- quality gates, escalation rules, and fallback behavior

<!-- rule-api:entry id=8ebcaf3f-e36e-4914-b11b-7b403846ac9d slug=shared/github-readme/github-copilot-configuration/active-files/supporting-documentation/l16 -->
### Supporting Documentation
**`COPILOT_INSTRUCTIONS_GUIDE.md`** - Meta-documentation about the custom instructions system
**`.github/instructions/*.instructions.md`** - Path-scoped guidance loaded by file pattern
Includes `viewer-api-tools.instructions.md` for viewer-api-driven frontend and API/file integration work.
**`.github/prompts/*.prompt.md`** - Explicit workflow prompts

<!-- rule-api:entry id=346cc9c9-73ef-4dc1-9c0d-b68395ddf249 slug=shared/github-readme/github-copilot-configuration/github-copilot-instruction-types/l22 -->
## GitHub Copilot Instruction Types

GitHub Copilot supports multiple instruction file types (in priority order):

<!-- rule-api:entry id=58d6157a-409a-4f21-8c36-a4ebea83c2da slug=shared/github-readme/github-copilot-configuration/github-copilot-instruction-types/l26 -->
1. **Agent instructions**: `AGENTS.md` ✅ **Primary in this repo**
   - Applied broadly for project-level conventions
   - Used as the single source of core behavior

<!-- rule-api:entry id=7024db8a-d420-49be-b16a-a6c2083a19a9 slug=shared/github-readme/github-copilot-configuration/github-copilot-instruction-types/l30 -->
2. **Path-specific**: `.github/instructions/*.instructions.md`
   - Target specific files/directories with glob patterns
   - Use for crate/tool/test specific guidance
   - Implemented in this repository

<!-- rule-api:entry id=065a7e28-3ced-45aa-a280-006745dda113 slug=shared/github-readme/github-copilot-configuration/github-copilot-instruction-types/l35 -->
3. **Prompt workflows**: `.github/prompts/*.prompt.md`
   - Named workflows invoked explicitly (for example, tickets, swarm-worker, debug-test, research)

<!-- rule-api:entry id=0a86d3ee-335d-4ac2-b212-0c7957a957a7 slug=shared/github-readme/github-copilot-configuration/github-copilot-instruction-types/l38 -->
4. **Model-specific**: `CLAUDE.md` or `GEMINI.md` (root only)
   - Provider-specific optimizations
   - Not used (provider-agnostic approach preferred)

<!-- rule-api:entry id=6dfa5c88-9e4b-4cba-8db4-72f916d8e032 slug=shared/github-readme/github-copilot-configuration/documentation-architecture/l42 -->
## Documentation Architecture

The custom instructions reference a layered documentation structure:

<!-- rule-api:entry id=5eee0939-d12a-498f-b5d0-8c7f18de0d84 slug=shared/github-readme/github-copilot-configuration/documentation-architecture/l46 -->
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

<!-- rule-api:entry id=406c2240-e652-4020-9cf6-8ae16edeabba slug=shared/github-readme/github-copilot-configuration/verification/l58 -->
## Verification

To verify Copilot is using these instructions:

<!-- rule-api:entry id=a14f0d51-2e3a-47eb-98da-46d1e574d672 slug=shared/github-readme/github-copilot-configuration/verification/l62 -->
**In VS Code:**
- Use Copilot Chat
- Check "References" in response
- Look for `AGENTS.md` and/or matching `.github/instructions/*.instructions.md`

<!-- rule-api:entry id=87c6008d-186f-4c68-ac8b-89ff25c94189 slug=shared/github-readme/github-copilot-configuration/verification/l67 -->
**On GitHub.com:**
- Use Copilot Chat or Agents
- Expand "References" at top of response
- Verify custom instructions file is listed

<!-- rule-api:entry id=b911e1bc-7cf9-4a7b-ab38-1119e6a31f21 slug=shared/github-readme/github-copilot-configuration/verification/l72 -->
**For Code Review:**
- Repository Settings → Copilot → Code review
- Verify "Use custom instructions" is enabled

<!-- rule-api:entry id=12a13109-50cb-4926-9de0-ace5d93a6897 slug=shared/github-readme/github-copilot-configuration/updates/l76 -->
## Updates

When updating instruction architecture:
- keep `AGENTS.md` concise and stable
- place domain rules in `.github/instructions/*.instructions.md`
- place task recipes in `.github/prompts/*.prompt.md`
- avoid duplicated copies of the same instruction content

<!-- rule-api:entry id=a28f4b3e-3620-40ed-8e5d-381290564642 slug=shared/github-readme/github-copilot-configuration/references/l84 -->
## References

- [GitHub Copilot Custom Instructions Docs](https://docs.github.com/en/copilot/customizing-copilot/adding-custom-instructions-for-github-copilot)
- [Custom Instructions Examples](https://docs.github.com/en/copilot/tutorials/customization-library/custom-instructions)
- [OpenAI agents.md](https://github.com/openai/agents.md)
