# manifest

Source: `crates/spec-api/src/manifest.rs`

## Public API

### `SpecManifest` (Struct)

A specification manifest — metadata about a spec stored in spec.toml.

Uses the same `extra: BTreeMap<String, Value>` storage pattern as
`EntityManifest` / `TicketManifest`. Spec-specific fields are stored in
the extra map and accessed via typed methods.

### `SpecManifest` (Impl)

