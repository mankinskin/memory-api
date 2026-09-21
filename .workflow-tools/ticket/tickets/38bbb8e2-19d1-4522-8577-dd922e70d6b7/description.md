# Objective

Migrate all existing specs in the workspace store to the aligned behavior-first structure.

# Scope

- migrate every spec body to the aligned contract shape
- ensure each migrated spec captures behavior story, provided contracts, required validation, and related implementation tickets
- preserve source intent by moving legacy rollout/worklog prose into compact background sections or linked ticket references
- keep entity details reference-first (no mandatory expanded payload duplication)
- run spec health validation across the full store after migration

# Acceptance Criteria

- all specs in `.spec/specs/` have the aligned top-level structure
- migrated specs explicitly include executable/operational validation steps and natural-language contract clauses
- code/schema/API references are included when available from existing content
- no spec health errors remain for migrated entries
- migration changes are reviewable and traceable to this ticket

# Validation

- full store spec health check
- targeted diffs for representative migrated specs
- no markdown diagnostics in touched files

# Restart Proof (2026-07-08)

- Workspace-wide spec-body coverage re-check passed after restart: 190 spec `body.md` files found across the root store, `memory-api`, `memory-viewers`, `viewer-api`, and the workspace fixture stores; 190 of 190 now carry `<!-- aligned-structure:v1 -->`.
- Full spec health re-check passed cleanly after the coverage repair: 190 specs checked, 0 issues.
- Restart validation uncovered three nested generated bodies that had been missed by the marker pass and were corrected in place:
  - `memory-api/.spec/specs/1cf68c36-7f64-4d81-b553-1947b978fbe3/body.md`
  - `memory-viewers/.spec/specs/6cf1685d-dc05-4022-abbb-efdd8e94af22/body.md`
  - `memory-viewers/.spec/specs/c5b11920-6ae8-4686-ae07-b8e9f8100bf8/body.md`
- Review scope remains limited to spec-store files plus the guidance surfaces from the linked guidance ticket; `.session/` capture directories remain outside the commit set.

# Commit Scope

- root store: `.spec/**`
- submodule stores: `memory-api/.spec/**` and `memory-viewers/.spec/**`
- paired guidance surfaces linked from ticket `37d7fac3`
- exclude all `.session/` capture directories from review and commit prep
