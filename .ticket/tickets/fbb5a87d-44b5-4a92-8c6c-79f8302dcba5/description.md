# spec-mcp: MCP Tool Surface

## Objective

Create MCP tools for spec-api, following the same pattern as ticket-mcp. Tools for creating, reading, updating, searching, and generating skills from specs.

## Tool List

| Tool | Description |
|------|-------------|
| `spec_create` | Create a new spec with title, slug, component, optional body |
| `spec_get` | Get spec by ID or slug, optionally with full sections |
| `spec_update` | Update fields, state, body, or sections |
| `spec_delete` | Soft-delete a spec |
| `spec_list` | List specs with filters (state, component, query) |
| `spec_search` | Full-text search across specs |
| `spec_tree` | Get hierarchy subtree for a spec |
| `spec_health` | Run health checks on specs |
| `spec_refs_validate` | Validate code references for a spec |
| `spec_skill_generate` | Generate SKILL.md for a crate or domain |
| `spec_toc` | Generate table of contents |

## Crate Structure

```
tools/mcp/spec-mcp/
├── Cargo.toml
└── src/
    ├── lib.rs
    └── tools/
```

## Acceptance Criteria

- [ ] All tools registered and callable via MCP protocol
- [ ] JSON schema for all tool inputs
- [ ] Error responses follow MCP conventions
- [ ] Integration with spec-api SpecStore