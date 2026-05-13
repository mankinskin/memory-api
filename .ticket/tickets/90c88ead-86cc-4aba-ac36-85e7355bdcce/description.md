# Slug System for spec-api

## Objective

Implement the slug validation and resolution system for specs. Slugs are hierarchical, human-readable identifiers (e.g. `ticket-api/storage/store`) that provide a user-friendly alternative to UUIDs.

## Slug Rules

- Hierarchical, separated by `/`
- Each segment: lowercase alphanumeric + hyphens, e.g. `ticket-api`
- Must be unique within the spec store
- Examples: `ticket-api/storage`, `context-read/algorithm/search`

## Implementation

1. `SlugValidator` — validate slug format (regex or manual parsing)
2. `SlugIndex` — in-memory `HashMap<String, Uuid>` for slug→UUID resolution
3. `SlugIndex::rebuild(manifests)` — rebuild index from a list of manifests
4. `SlugIndex::insert(slug, id)` → error if duplicate
5. `SlugIndex::resolve(slug)` → `Option<Uuid>`
6. `SlugIndex::remove(slug)`

## Acceptance Criteria

- [ ] `validate_slug()` accepts valid slugs and rejects invalid ones
- [ ] SlugIndex resolves slug→UUID
- [ ] SlugIndex enforces uniqueness (returns error on duplicate insert)
- [ ] SlugIndex::rebuild() constructs index from manifest list
- [ ] Unit tests for validation edge cases (empty, trailing slash, uppercase, special chars)
