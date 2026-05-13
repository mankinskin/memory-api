# Generate Initial Skill Files

## Objective

Use the skill generation engine to produce the first set of SKILL.md files covering all ticket system tools.

## Target Skills

| Skill File | Source Specs | Scope |
|------------|-------------|-------|
| `docs/skills/ticket-api.md` | ticket-api/* specs | Per-crate |
| `docs/skills/ticket-cli.md` | ticket-cli spec | Per-crate |
| `docs/skills/ticket-http.md` | ticket-http spec | Per-crate |
| `docs/skills/ticket-mcp.md` | ticket-mcp spec | Per-crate |
| `docs/skills/ticket-vscode.md` | ticket-vscode spec | Per-crate |
| `docs/skills/ticket-system.md` | All ticket specs | Per-domain |
| `docs/skills/INDEX.md` | All specs | Master index |

## Acceptance Criteria

- [ ] All skill files above generated and written to `docs/skills/`
- [ ] Each skill includes: overview, public API, common operations, pitfalls, test examples
- [ ] Domain skill aggregates per-crate content
- [ ] INDEX.md lists all skills with coverage metrics
- [ ] Skills validated against spec data (no stale content)