# state

Source: `tools/http/spec-http/src/state.rs`

## Public API

### `SpecAppState` (Struct)

Shared application state for spec-http handlers.

SpecStore needs `&mut self` for create/update/delete/scan,
so we wrap it in an async Mutex. The Mutex is held only for
the duration of each handler call.

### `SpecAppState` (Impl)

