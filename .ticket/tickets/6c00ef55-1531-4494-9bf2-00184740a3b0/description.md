# Skill Generation: Master Index

## Objective

Generate a master `docs/skills/INDEX.md` that serves as the entry point for all generated skill files, with coverage statistics and cross-references.

## Content

```markdown
# Skill Index

Generated: 2026-04-18T15:00:00Z

## Per-Crate Skills
| Crate | Skill File | Specs | Coverage | Last Updated |
|-------|-----------|-------|----------|--------------|
| ticket-api | [ticket-api.md](ticket-api.md) | 14 | 85% | 2026-04-18 |
| ...

## Per-Domain Skills  
| Domain | Skill File | Crates | Specs |
|--------|-----------|--------|-------|
| ticket-system | [ticket-system.md](ticket-system.md) | 5 | 42 |

## Coverage Gaps
- ticket-api/execution: 0% coverage (no specs)
- ...
```

## Acceptance Criteria

- [ ] INDEX.md generated with per-crate and per-domain tables
- [ ] Coverage percentage calculated from spec feature status
- [ ] Coverage gaps highlighted
- [ ] Regenerated on `spec skill generate --all`