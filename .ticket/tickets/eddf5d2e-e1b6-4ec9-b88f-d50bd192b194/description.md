# Skill Generation Engine

## Objective

Build a skill file generation engine that reads spec data from the SpecStore and produces structured SKILL.md files for AI coding agents.

## Output Structure

### Per-Crate Skills (e.g. `docs/skills/ticket-api.md`)

```markdown
# ticket-api Skill

## Overview
[Generated from spec body]

## Public API
[Generated from code refs — list of structs/traits/functions with descriptions]

## Common Operations
[Generated from spec sections — concrete code examples]

## Pitfalls & Gotchas
[Generated from spec features.bugs + features.blocked]

## Test Examples
[Generated from test matrix or existing test files]

## Related Specs
[Links to child specs and linked specs]
```

### Per-Domain Skills (e.g. `docs/skills/ticket-system.md`)

Aggregates per-crate skills into a domain overview covering:
- Architecture diagram (from domain root spec)
- Component relationships
- Cross-crate workflows
- Domain-wide pitfalls

### Master Index (`docs/skills/INDEX.md`)

Table of all generated skills with:
- Crate/domain name
- Spec coverage percentage
- Last generated timestamp
- Link to skill file

## Generation Logic

1. Query SpecStore for all specs matching target component/domain
2. For each spec: extract title, body, code_refs, features, sections
3. Render into SKILL.md template
4. Validate: check that all code_refs still resolve, flag stale content
5. Write to `docs/skills/` directory

## Acceptance Criteria

- [ ] `spec skill generate --crate ticket-api` produces valid SKILL.md
- [ ] Per-domain rollup skill includes all crate skills in domain
- [ ] Master INDEX.md lists all generated skills with coverage stats
- [ ] Generated files include concrete use-case examples from specs
- [ ] Pitfall notes pulled from features.bugs and features.blocked
- [ ] Stale content flagged with warnings in generated output