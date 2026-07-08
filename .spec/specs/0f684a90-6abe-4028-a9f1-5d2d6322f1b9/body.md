<!-- aligned-structure:v1 -->

# Summary

Source: `tools/http/spec-http/src/state.rs`

## Behavior Story

Source: `tools/http/spec-http/src/state.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# state

Source: `tools/http/spec-http/src/state.rs`

## Public API

### `SpecAppState` (Struct)

Shared application state for spec-http handlers.

SpecStore needs `&mut self` for create/update/delete/scan,
so we wrap it in an async Mutex. The Mutex is held only for
the duration of each handler call.

### `SpecAppState` (Impl)
