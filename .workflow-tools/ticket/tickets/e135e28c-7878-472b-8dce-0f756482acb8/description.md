# T4: Document Operations — Merge, Split, Page Edits, Metadata

## Objective

Implement the four document-manipulation variants. These are the write-path
operations, so they are the primary consumers of the T2 safety layer.

## Files

- `memory-api/crates/pdf-api/src/document.rs` (new; or split into
  `merge.rs` / `split.rs` / `pages.rs` / `metadata.rs` if it grows past a
  comfortable single-file size)
- `memory-api/crates/pdf-api/src/lib.rs` (wire into `execute()`)
- `memory-api/crates/pdf-api/tests/document.rs` (new)

## Design

Use the crate T0 bound to capabilities 2–4 (expected to be the low-level
structural crate). T0's Findings are authoritative for the API.

### Merge (N → 1)

- `inputs: Vec<PathBuf>`, `output: WriteTarget`.
- Result page order follows `inputs` order.
- Every input validated against the sandbox root before any work starts.
- Empty `inputs` is a user error (`EmptyInputSet`).
- A single input is legal and behaves as a copy.
- Object-ID collisions across source documents must be renumbered correctly —
  this is the classic PDF merge bug. Assert it with a fixture whose sources have
  overlapping object IDs and verify the merged document still opens and all
  pages render their own content.

### Split (1 → N)

- `input`, `output_dir`, and a split spec: per-page, or explicit ranges.
- Response returns the full ordered list of written paths.
- Validate-all-then-write: every destination is root-checked and
  write-policy-checked before the first byte is written, so a rejection cannot
  leave a partial output set.
- A naming template with a documented default (e.g. `{stem}-{page:03}.pdf`).
- Filename collisions between produced outputs are a user error.

### Page edits

Ops: delete, reorder, rotate. Applied to `input`, written to `output`.

**T0 confirmed (reading `lopdf` 0.44.0 `src/processor.rs` and the full `src/`
tree): `Document::delete_pages(&mut self, &[u32])` exists, but there is no
reorder method and no rotate method anywhere in the crate.** Delete uses the
native API. Reorder and rotate are hand-rolled page-tree edits: reorder by
rewriting the page-tree `Kids` array to the requested permutation of existing
page object refs, and rotate by setting each affected page dictionary's
`/Rotate` entry directly. Budget implementation effort for both as low-level
structural edits, not thin wrappers over a library call.

- Reorder must be a permutation check — a spec that drops or duplicates pages
  unintentionally should be rejected unless explicitly expressed as a delete.
- Deleting every page is a user error (a zero-page PDF is not a valid document).
- Rotation restricted to multiples of 90.
- Page indices: **do not choose a convention here.** T2 fixes it for the whole
  crate — external surfaces are 1-based inclusive, internal Rust indices are
  0-based, and conversion happens only at request validation in `pdf-api`.
  Follow that rule; do not add a second conversion point in this ticket.

### Metadata

- `GetMetadata` is read-only; returns the standard document info fields
  (title, author, subject, keywords, creator, producer, creation/mod dates).
- `SetMetadata` takes a field map, writes to `output`.
- A field set to null/empty must be distinguishable from a field left untouched
  — decide the semantic (explicit clear vs no-op), document it, and test both.
- Non-UTF8 / oddly-encoded metadata strings must not panic.

## Acceptance Criteria

- [ ] Merging two PDFs produces a document with the sum of the page counts, in
      input order, that opens cleanly.
- [ ] Merging sources with colliding object IDs produces a valid document with
      correct per-page content.
- [ ] Merge with zero inputs is a user error; with one input it copies.
- [ ] Split per-page produces exactly N files and returns all N paths.
- [ ] Split with explicit ranges produces the requested documents.
- [ ] A split rejected partway through writes **no** files.
- [ ] Page delete / reorder / rotate each produce correct output.
- [ ] Deleting all pages is a user error.
- [ ] Non-multiple-of-90 rotation is a user error.
- [ ] Page indexing follows T2's convention (1-based external, 0-based internal,
      single conversion point) with no additional conversion introduced here.
- [ ] Metadata round-trips: set then get returns what was set.
- [ ] Clear-vs-untouched metadata semantics are documented and tested.
- [ ] Every write path honors copy-on-write and the `overwrite` flag from T2.
- [ ] In-place operation on the input file leaves the original intact if the
      operation fails.

## Validation

```bash
cargo test -p pdf-api document
```

## Depends On

T2.
