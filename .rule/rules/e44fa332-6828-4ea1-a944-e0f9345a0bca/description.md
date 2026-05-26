## Inputs accepted

- A full canonical UUID (lower-case, hyphenated): always matches exactly one entity if it exists.
- A prefix of the UUID (4 characters or more): must resolve to exactly one entity in the store, otherwise the resolver fails with `code: not_found` (no matches) or `code: conflict` (multiple matches).
- A hierarchical slug (specs and rules only): matches the entity whose `slug` field equals the input.