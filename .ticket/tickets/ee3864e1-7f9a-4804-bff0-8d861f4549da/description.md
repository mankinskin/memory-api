# Epic: Specification System

## Vision

Build a complete specification and documentation management system that:
1. Stores ground-truth specification files in a filesystem-backed database (same architecture as ticket-api)
2. Tracks specs through a full lifecycle: draft → reviewed → approved → implemented → verified
3. Links specs to implementation code at symbol-level granularity
4. Generates SKILL.md files for AI agents with up-to-date interface docs, use-case examples, and pitfall notes
5. Integrates with the ticket system for tracking refinement, validation, and bugfix work

## Architecture

```
memory-api (new, extracted from ticket-api)
├── Generic entity manifest, schema engine, state machine
├── Filesystem entity store (folder-per-entity, manifest.toml, history.ndjson)
├── Redb metadata index + Tantivy full-text search
├── Edge system (directed/undirected, acyclicity enforcement)
├── Workspace resolution, filesystem watcher, board coordination
└── Query language (field:value + FTS)

ticket-api (refactored to depend on memory-api)
├── TicketStore wrapping memory-api EntityStore
├── tracker-improvement schema, default state machine
├── Validation/release protocol, git history tracking
└── Execution layer (sandbox, Copilot provider)

spec-api (new, depends on memory-api)
├── SpecManifest (multi-file: spec.toml + body.md + sections/*.md + assets/)
├── UUID + human-readable slug identity
├── Parent-child hierarchy (stored in each node)
├── Symbol-level code references with line ranges
├── Specification schema (draft/reviewed/approved/implemented/verified)
├── Feature completeness tracking, spec health checks
├── Skill file generation engine
└── Test stub + test matrix generation

spec-cli / spec-mcp / spec-http / spec-vscode (thin interface layers)
```

## Phases

| Phase | Focus | Priority |
|-------|-------|----------|
| P0 | Extract memory-api, refactor ticket-api | critical |
| P1 | spec-api core: model, schema, storage, code refs | high |
| P2 | Interface layers: CLI, MCP, HTTP | high |
| P3 | Spec creation and code sync | high |
| P4 | Validation, skill generation, test generation | high-medium |
| P5 | Ticket integration, cross-entity edges | medium |
| P6 | Search, TOC, hierarchical DAG | medium |
| P7 | spec-vscode extension | low |
| P8 | Bootstrap: write initial specs + generate skills | high |

## Design Decisions

- **Same workspace**: Specs and tickets share the same index root, different entity types
- **UUID + slug**: UUID on disk, mandatory unique slug for human references
- **Multi-file folders**: spec.toml + body.md + sections/*.md + assets/ per spec
- **Symbol-level code refs**: Link to functions/structs/traits with line ranges
- **Parent-child in node**: Hierarchy stored as `parent` field in each spec manifest
- **Extract first**: Build memory-api before spec-api to avoid duplication
- **Separate CLI**: `spec` binary, not a subcommand of `ticket`
- **Per-crate + domain + index skills**: SKILL.md files at multiple granularities

## Acceptance Criteria

- [ ] memory-api crate extracted with ticket-api passing all existing tests
- [ ] spec-api crate with working CRUD, schema validation, and parent-child hierarchy
- [ ] spec-cli with create/get/update/list/search/health commands
- [ ] Spec files created for all ticket system crates
- [ ] Skill files generated for ticket-api, ticket-cli, ticket-http, ticket-mcp
- [ ] Code references link specs to implementation at symbol level
- [ ] Spec health check validates completeness and reference integrity