# Refactor ticket-api to Depend on memory-api

## Objective

After memory-api is extracted, refactor ticket-api to be a thin domain layer on top of memory-api, keeping only ticket-specific logic.

## Refactored ticket-api Structure

```
ticket-api/
├── Cargo.toml          (depends on memory-api)
├── schemas/
│   └── tracker-improvement.toml
└── src/
    ├── lib.rs           (re-exports memory-api types + ticket-specific API)
    ├── error.rs         (ProtocolError only; StorageError re-exported from memory-api)
    ├── store.rs         (TicketStore wrapping memory_api::EntityStore + domain methods)
    ├── default_schema.rs
    ├── event.rs         (git history tracking)
    ├── contracts/       (TicketCommand, command schema export)
    └── execution/       (sandbox, Copilot provider, assignment runner)
```

## Key Changes

1. Replace all generic types with memory-api re-exports
2. `TicketStore` wraps `memory_api::EntityStore` and adds:
   - `validate_start()` / `validate_result()`
   - `release_candidate_create()` / `release_gate_check()` / `release_promote()`
   - `close()` (fast-forward through state machine)
3. `TicketManifest` becomes a type alias or thin wrapper around `memory_api::EntityManifest`
4. All existing tests must continue passing without modification
5. All downstream crates (ticket-cli, ticket-http, ticket-mcp) must compile

## Acceptance Criteria

- [ ] ticket-api depends on memory-api in Cargo.toml
- [ ] All generic code removed from ticket-api (no duplication)
- [ ] `cargo test -p ticket-api` passes
- [ ] `cargo test -p ticket-cli` passes
- [ ] `cargo test -p ticket-http` passes
- [ ] `cargo test -p ticket-mcp` passes
- [ ] Public API surface of ticket-api unchanged for downstream consumers