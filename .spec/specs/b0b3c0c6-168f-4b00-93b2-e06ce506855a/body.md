# slug

Source: `crates/spec-api/src/slug.rs`

## Public API

### `validate_slug` (Function)

Validate a slug string.

Rules:
- Must not be empty
- Segments separated by `/`
- Each segment: lowercase `[a-z0-9]` and hyphens `-`
- No empty segments (no `//`, no leading/trailing `/`)
- No uppercase letters
- No special chars other than `-` and `/`

### `SlugIndex` (Struct)

In-memory slug → UUID index with uniqueness enforcement.

### `SlugIndex` (Impl)

