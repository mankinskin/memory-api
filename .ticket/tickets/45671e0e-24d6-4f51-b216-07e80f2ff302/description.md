# Test Generation

## Objective

Generate Rust test stubs for uncovered spec features and a test matrix checklist linking existing tests to spec acceptance criteria.

## Rust Test Stubs

For each spec with acceptance criteria, generate:

```rust
/// Validates: [spec-slug] — [acceptance criterion text]
/// Spec: [spec-id]
#[test]
fn test_spec_ticket_api_store_create() {
    todo!("Implement test for: TicketStore::create returns new ticket with generated UUID")
}
```

Output to `tests/generated/spec_<component>.rs`

## Test Matrix

Markdown checklist mapping specs to existing tests:

```markdown
# Test Matrix: ticket-api

## TicketStore (spec: ticket-api/storage/store)
- [x] create — `tests/integration_workflow_crud.rs::test_create_ticket`
- [x] get — `tests/integration_workflow_crud.rs::test_get_ticket`  
- [ ] bulk_update — **NO TEST** (planned feature)
- [x] delete — `tests/integration_workflow_crud.rs::test_delete_ticket`
```

## CLI Interface

```
spec test generate [--crate <name>] [--all] [--dry-run]
spec test matrix [--crate <name>] [--all]
```

## Acceptance Criteria

- [ ] Test stubs generated with spec references in doc comments
- [ ] Test matrix links existing tests to spec features
- [ ] Missing coverage highlighted in matrix
- [ ] Dry-run mode shows what would be generated