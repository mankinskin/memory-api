Ticket creation (`TicketStore::create`) previously validated against the type's schema only when one was registered, silently persisting tickets whose `type` had no schema. The failure only surfaced later, at transition resolution, far from the actual mistake.

**Resolution (user-selected, not schema registration for `ticket`):** creation now resolves the schema unconditionally and fails loudly via a new `SchemaValidationError::UnknownType` error naming the offending type and listing all registered type ids, so the caller can self-correct immediately. No schema was registered for type `"ticket"` — that option was explicitly rejected.

**Out of scope (explicit non-goals):** no migration or retyping of the four existing tickets with unregistered types (`bba9b313`, `33463861`, `7c74f2fe`, `1ff57502`) — per the user, "Leave them, they are already retyped." No bypass/force flag was added.

**Blast radius fixed:** test/fixture call sites creating tickets with unregistered type ids (`"nested-ticket"`, `"made-up-type"`) were updated to use a registered type (`tracker-improvement`); the health.rs unregistered-type regression test now simulates legacy data by writing the manifest to disk directly and reindexing, since `create()` itself no longer permits creating such a ticket.

**Tests added:** `create_rejects_unregistered_type_naming_offender_and_registered_types` and `create_still_succeeds_for_registered_type` in `memory-api/crates/ticket-api/src/storage/store_tests.rs`.

**Validation:** `cargo test -p ticket-api` — 206 passed, 0 failed (memory-api submodule).