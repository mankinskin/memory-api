# store

Source:

- `crates/rule-api/src/store.rs`
- `crates/rule-api/src/manifest.rs`
- `crates/memory-api/src/model/filesystem.rs`
- `crates/memory-api/src/storage/entity_fs.rs`

## Summary

`rule-api` stores rule metadata and rule prose separately. Canonical rule body
content must live in `body.md`, while `rule.toml` remains the metadata manifest.
Legacy rule folders that still use `description.md` or a manifest-level `body`
field remain readable during migration, but they are compatibility inputs rather
than the long-term storage contract.

## Canonical Rule Folder Layout

Each rule entry folder should converge on this shape:

- `.rule/rules/<uuid>/rule.toml`
- `.rule/rules/<uuid>/body.md`
- `.rule/rules/<uuid>/history.ndjson`
- optional `.rule/rules/<uuid>/assets/`
- optional `.rule/rules/<uuid>/feedback/`

`rule.toml` owns structured metadata such as `id`, `slug`, `title`, `state`,
`file_kind`, `section`, ordering, repo/path scopes, source provenance, and
feedback summary fields. The manifest should not be required to persist the full
markdown body as duplicated prose.

## Shared Storage Seam

The generic entity filesystem contract should let each domain choose its body
filename instead of hardcoding one global markdown asset name.

- `rule-api` should use `body.md`.
- `spec-api` already uses `body.md`.
- `ticket-api` continues to use `description.md`.

This keeps the shared storage layer reusable while allowing each domain to keep
its canonical authored surface stable.

## Read And Write Contract

### Writes

- Newly created rule entries write canonical prose to `body.md`.
- Rule body updates rewrite `body.md` and keep history/search/index state in
  sync with the hydrated body text.
- Rule generation and query flows should never require authors to edit
  manifest-level `body` text directly.

### Reads

Rule hydration must prefer the filesystem body asset over duplicated metadata:

1. read `body.md` when present
2. otherwise read legacy `description.md`
3. otherwise read manifest `body` only as a compatibility fallback

CLI, MCP, search, explain, and markdown-rendering surfaces should expose the
same hydrated body regardless of whether the backing rule folder is canonical or
legacy during the migration window.

## Compatibility And Migration

During migration, `rule-api` must continue to scan and open legacy rule folders
that contain `description.md` and/or a manifest-level `body` field.

Bulk migration must treat conflicting legacy sources as a blocking data issue:

- if `description.md` and manifest `body` match, either source may seed
  canonical `body.md`
- if only one source exists, that source seeds canonical `body.md`
- if both exist and differ, migration tooling must stop and report the folder
  instead of choosing one implicitly

The repository rollout should proceed in this order:

1. update the storage contract spec
2. land compatibility-aware storage code
3. backfill existing rule folders in the root workspace, `memory-viewers`,
   `memory-api`, and `viewer-api`

## Validation

The implementation should add or keep focused validation for:

- canonical create/get/update behavior against `body.md`
- compatibility reads for legacy `description.md` rule folders
- compatibility reads for manifest-only `body` data when no body file exists
- rule-cli and rule-mcp behavior that surfaces hydrated body content
- post-migration checks that detect remaining duplicated manifest bodies or
  unmigrated rule folders

## Non-Goals

- Renaming ticket or other non-rule entity description files.
- Changing target composition rules beyond the source of hydrated rule body
  content.
- Removing compatibility for legacy rule folders before the repository backfill
  is complete.