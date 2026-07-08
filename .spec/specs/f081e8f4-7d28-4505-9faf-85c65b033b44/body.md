<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/storage/entity_fs.rs`

## Behavior Story

Source: `crates/memory-api/src/storage/entity_fs.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# entity_fs

Source: `crates/memory-api/src/storage/entity_fs.rs`

## Public API

### `HistoryRevision` (Struct)

A single immutable revision snapshot stored in `history.ndjson`.

Revisions are append-only; `revert` creates a new revision with old state.

### `EntityScanEntry` (Struct)

### `EntityFs` (Struct)

Generic filesystem operations for entity folders.

Each entity lives in a folder named by its UUID:

```text
<scan_root>/<uuid>/
<manifest_file>     ← manifest (TOML), e.g. ticket.toml or spec.toml
<lock_file>         ← advisory lock file during writes
assets/             ← optional attachments
history.ndjson      ← append-only revision log
```

Configure the manifest and lock filenames via [`EntityFolderConfig`].

### `EntityFs` (Impl)
