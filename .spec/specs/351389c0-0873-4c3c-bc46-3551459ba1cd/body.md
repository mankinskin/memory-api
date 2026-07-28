## Motivation

`SpecStore::update_body` previously accepted any content unconditionally, including an accidental empty write or a byte-identical no-op write that silently discarded intent. This spec documents the implemented guard: `update_body` rejects both cases with actionable errors unless the caller explicitly opts into an empty write via `force`.

## Dependent expectation

If this spec is implemented, dependents (spec-mcp, spec-cli, and any future transport) can rely on:
- `update_body(id_or_slug, content, force)` rejects empty `content` with `SpecError::EmptyBody` unless `force` is `true`.
- `update_body` rejects `content` that is byte-identical to the existing stored body with `SpecError::NoOpUpdate`, regardless of `force`.
- **A successful `update_body` call guarantees the stored body actually changed** — either it errored, or the body is different from what it was before the call.
- Both rejections carry distinct, actionable error messages that name the target and how to proceed (force flag for empty, or supply different content for no-op).

## Provided Surface Contracts

### Core (`memory-api/crates/spec-api/src/store.rs`)
- `SpecStore::update_body(&self, id_or_slug: &str, content: &str, force: bool) -> Result<(), SpecError>` — [store.rs L691-711](memory-api/crates/spec-api/src/store.rs#L691-L711)
  - Empty-content rejection unless `force`: [store.rs L703-705](memory-api/crates/spec-api/src/store.rs#L703-L705)
  - Byte-identical no-op rejection: [store.rs L706-709](memory-api/crates/spec-api/src/store.rs#L706-L709)

### Errors (`memory-api/crates/spec-api/src/error.rs`)
- `SpecError::EmptyBody(String)` — `"empty body update rejected for {0} (pass force=true to allow)"` — [error.rs L17-18](memory-api/crates/spec-api/src/error.rs#L17-L18)
- `SpecError::NoOpUpdate(String)` — `"no-op body update rejected for {0}: content is unchanged"` — [error.rs L20-21](memory-api/crates/spec-api/src/error.rs#L20-L21)

### Transport surfaces
- MCP: `UpdateSpecInput.force_body: bool` — [spec-mcp types.rs L71-73](memory-api/tools/mcp/spec-mcp/src/server/types.rs#L71-L73); used to call `update_body` when `body` is supplied — [spec-mcp query.rs L145-148](memory-api/tools/mcp/spec-mcp/src/server/query.rs#L145-L148); `EmptyBody`/`NoOpUpdate` mapped to `invalid_params` (not `internal_error`) — [spec-mcp server.rs L94-107](memory-api/tools/mcp/spec-mcp/src/server.rs#L94-L107)
- CLI: `--force-body` — [spec-cli args.rs L67-69](memory-api/tools/cli/spec-cli/src/cli/args.rs#L67-L69); wired into `update_body` when `--body-file` is supplied — [spec-cli crud.rs L129-133](memory-api/tools/cli/spec-cli/src/cli/commands/crud.rs#L129-L133)

## Non-Goals

- Does not add a diff/merge mode for body updates; a change is still a full-content replace, only gated against empty/no-op writes.
- Does not change slug, manifest field, or section update semantics — only `body.md` content writes.

## Required Validation

- `update_body_rejects_empty_content_without_force` — [store/tests.rs L117](memory-api/crates/spec-api/src/store/tests.rs#L117)
- `update_body_allows_empty_content_with_force` — [store/tests.rs L130](memory-api/crates/spec-api/src/store/tests.rs#L130)
- `update_body_rejects_noop_content` — [store/tests.rs L141](memory-api/crates/spec-api/src/store/tests.rs#L141)
- `update_body_succeeds_on_genuine_change` — [store/tests.rs L153](memory-api/crates/spec-api/src/store/tests.rs#L153)
- Command: `rtk cargo test -p spec-api` → 79 passed, 0 failed
- Landed in memory-api submodule commit `cb423c5`

## Related Implementation Tickets

- [f986e666 [spec-api] Reject empty and no-op spec body updates so a successful update guarantees a change](c:/Users/linus/git/graph_app/context-engine/.ticket/tickets/f986e666-d8db-4845-ba86-eb4bb89484ce) — state: in-review, blocked on this spec for the in-review→done traceability gate.

## Related Specs

- `spec-api/error` ([83094beb](c:/Users/linus/git/graph_app/context-engine/.spec/specs/83094beb-d315-4b16-b132-3ae22a528422)) documents the broader `SpecError` enum; `EmptyBody`/`NoOpUpdate` are cross-linked here since they gate this specific behavior.

## Legacy Content (Preserved)

# store

Source: `crates/spec-api/src/store.rs`

## Public API

### `SpecStore` (Struct)

The central spec store: wraps `EntityStore` with spec-specific features.

Adds slug uniqueness enforcement, `body.md` management, `sections/` CRUD,
and parent-child hierarchy traversal on top of the generic entity store.

### `SpecStore` (Impl)

## Create Root Resolution

`SpecStore::create` accepts either an explicit registered scan root or a local
workspace path that resolves to the canonical `.spec` store.

When callers pass a workspace root, the `.spec` store root, or a path inside
that store, creation must normalize the destination to `.spec/specs` before the
entity folder is created.

Paths outside the registered scan roots and outside the local `.spec` store
must be rejected with a storage error instead of writing spec folders into the
caller-provided directory.
