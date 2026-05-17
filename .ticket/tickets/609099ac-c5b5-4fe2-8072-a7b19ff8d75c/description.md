# Summary

Add `cargo metadata` output support to the new docs surface so `doc-api` can use Cargo's workspace and package graph as a docs workspace input.

## Acceptance Criteria

- `crates/doc-api` can ingest `cargo metadata` output from a `cargo_metadata::Metadata` value.
- `crates/doc-api` can ingest the same metadata from a JSON string or JSON file.
- The docs workspace model records workspace root, workspace manifest path, target directory, workspace packages, and doc-capable targets.
- Focused tests cover a multi-package workspace with both library and binary targets.
