# Multi-File Spec Folder Structure

## Objective

Extend the EntityFs pattern to support the multi-file spec folder layout. Each spec lives in a `<scan_root>/<uuid>/` directory with a defined set of files.

## Folder Layout

```
<scan_root>/<uuid>/
  spec.toml             ← manifest (SpecManifest)
  body.md               ← main specification body
  sections/             ← optional sub-sections (overview.md, api-surface.md, etc.)
  assets/               ← diagrams, images
  .spec-lock            ← advisory lock (fs4 exclusive)
  history.ndjson        ← append-only revision log
```

## Implementation

1. Define folder layout constants (file names, directory names)
2. Implement `SpecFs` or extend `EntityFs` to:
   - Create spec folders with spec.toml + body.md
   - Read spec.toml → SpecManifest
   - Read/write body.md
   - List/read/write sections/*.md
   - List assets/
3. Manifest file is `spec.toml` (not `ticket.toml`)

## Acceptance Criteria

- [ ] Spec folder created with spec.toml + body.md on create
- [ ] sections/ and assets/ directories created on demand
- [ ] Read spec.toml → SpecManifest
- [ ] Read/write body.md
- [ ] history.ndjson appended on changes
- [ ] .spec-lock advisory locking via fs4
- [ ] Round-trip test: create folder → read back → matches
