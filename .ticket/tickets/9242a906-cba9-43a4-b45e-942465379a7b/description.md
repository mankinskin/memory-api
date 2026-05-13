# Bootstrap: ticket-api Spec Files

## Objective

Write comprehensive spec files documenting the ticket-api crate's full API surface, storage layer, schema system, and edge system.

## Specs to Create

| Slug | Scope | Code Target |
|------|-------|-------------|
| `ticket-api` | crate | Root overview of ticket-api |
| `ticket-api/model/ticket` | type | TicketManifest, TicketId |
| `ticket-api/model/schema` | module | TicketTypeSchema, SchemaRegistry |
| `ticket-api/model/edge` | module | EdgeRecord, EdgeKindRule, EdgeRegistry |
| `ticket-api/model/query` | module | Expr, parse_query |
| `ticket-api/storage/store` | module | TicketStore full API |
| `ticket-api/storage/ticket-fs` | module | Filesystem operations |
| `ticket-api/storage/index` | module | RedbIndexStore |
| `ticket-api/storage/search` | module | TantivySearchIndex |
| `ticket-api/storage/board` | module | Board coordination |
| `ticket-api/workspace` | module | Workspace resolution |
| `ticket-api/watcher` | module | Filesystem watcher |
| `ticket-api/contracts` | module | Command schema |
| `ticket-api/execution` | module | Sandbox, provider |

## Acceptance Criteria

- [ ] All specs above created with correct slugs
- [ ] Code refs pointing to actual source files and symbols
- [ ] Feature lists reflecting current implementation status
- [ ] Parent-child hierarchy: ticket-api → module specs
- [ ] Body content accurate to current implementation