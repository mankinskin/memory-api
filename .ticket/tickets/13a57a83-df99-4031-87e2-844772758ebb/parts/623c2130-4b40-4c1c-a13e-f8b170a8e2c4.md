# Bootstrap Spec Files for the Spec System

Author the canonical specification database covering the spec-system crates
(`memory-api`, `spec-api`, `spec-cli`, `spec-mcp`, `spec-http`).

## Current State (Phase 1 complete — 2026-04-20)

`spec bootstrap` has been run on all five crates; **38 specs** exist in
`.spec/specs/`, all in state `draft`, with valid CodeRefs and zero health
issues:

| Crate         | Root + module specs |
|---------------|--------------------:|
| `memory-api`  | 1 + 16              |
| `spec-api`    | 1 + 6               |
| `spec-cli`    | 1 + 3               |
| `spec-mcp`    | 1 + 1               |
| `spec-http`   | 1 + 7               |

A slug-normalization bug was fixed in
[tools/cli/spec-cli/src/cli/commands/bootstrap.rs](tools/cli/spec-cli/src/cli/commands/bootstrap.rs):
module names with `_` were generating invalid slugs; bootstrap now rewrites
`_` → `-`.

What is missing:
- Bodies are auto-generated stubs ("Bootstrapped from source analysis…").
- No cross-crate `spec-system` umbrella spec.
- No `sections/` (`design.md`, `acceptance.md`) on any spec.
- All specs still `draft`; no `reviewed`/`approved`/`implemented` history.
- Some bootstrap-generated CodeRefs are too coarse — e.g. the `SpecStore` impl
  block CodeRef spans lines 32–480 of `store.rs`, a single ref covering the
  entire impl. These need replacement during hand-curation.

## Goal

Replace stub bodies with hand-curated content (purpose, key types, invariants,
pitfalls); add `acceptance.md` sections to high-priority specs; advance them
through the state machine to `approved` (or `implemented` where the code has
already merged).

## Scope (in)

1. Author a `spec-system` umbrella root spec with architecture diagram.
2. Hand-curate bodies for the **5 crate roots** + **15 priority module specs**
   (Priority A and B tables below).
3. Add `acceptance.md` sections to every Priority A + B spec.
4. Add `design.md` sections to every Priority A spec.
5. Replace coarse impl-block CodeRefs with per-method refs **in the same
   file** for Priority A specs.
6. Advance Priority A + B specs through `reviewed → approved`. Where the
   referenced code is already merged, advance to `implemented`.

## Scope (out)

- `ticket-api` and `context-*` specs — separate ticket
  [9242a906](.ticket/tickets/9242a906-cba9-43a4-b45e-942465379a7b/).
- Skill generation from spec data — ticket
  [eddf5d2e](.ticket/tickets/eddf5d2e-e1b6-4ec9-b88f-d50bd192b194/).
- Spec-to-code sync (auto line-number updates) — ticket
  [80e25216](.ticket/tickets/80e25216-7ba9-4fd9-bc80-3311f1d2a604/).
- **Cross-component `linked` edges** — blocked. See "Resolved Risks" below.
  Carved out into a follow-up ticket.

## Phases

### Phase 1 — Bootstrap (DONE)

- [x] Add scan root and run `spec bootstrap` on all five crates
- [x] Fix slug normalization bug (`_` → `-`)
- [x] Verify `spec health --all` is clean
- [x] Verify CodeRefs validate via `spec refs <id> validate`

Output: 38 specs in `.spec/specs/`.

### Phase 2 — Umbrella spec

Create the `spec-system` root spec covering the whole domain:

```bash
./target/debug/spec.exe create \
  --title "Spec System" \
  --slug spec-system \
  --component spec-system \
  --scope domain \
  --body-file humans/tmp/spec-bodies/spec-system.md \
  --json
```

Body must contain:
- One-paragraph elevator pitch (link spec ↔ code, hierarchy, multi-transport).
- Mermaid architecture diagram: `memory-api → spec-api → {spec-cli, spec-mcp, spec-http}`.
- Reference to the state machine in
  [crates/spec-api/schemas/specification.toml](crates/spec-api/schemas/specification.toml).
- On-disk layout description.

Then re-parent the five crate-root specs by setting their `parent` manifest field:

```bash
./target/debug/spec.exe update memory-api  --field 'parent=<spec-system-uuid>' --json
./target/debug/spec.exe update spec-api    --field 'parent=<spec-system-uuid>' --json
./target/debug/spec.exe update spec-cli    --field 'parent=<spec-system-uuid>' --json
./target/debug/spec.exe update spec-mcp    --field 'parent=<spec-system-uuid>' --json
./target/debug/spec.exe update spec-http   --field 'parent=<spec-system-uuid>' --json
```

**Verification:** `spec tree` must show the five crates as children of `spec-system`.

### Phase 3 — Hand-curate bodies (Priority A: 8 specs)

| # | Slug                                     | Notes |
|---|------------------------------------------|-------|
| 1 | `memory-api`                             | Foundation for both spec-api and ticket-api |
| 2 | `memory-api/storage/entity-store`        | Generic store contract |
| 3 | `memory-api/model/schema`                | Schema registry mechanism |
| 4 | `spec-api`                               | Crate overview |
| 5 | `spec-api/manifest`                      | `SpecManifest` contract |
| 6 | `spec-api/store`                         | `SpecStore`, slug uniqueness, body/sections handling |
| 7 | `spec-api/code-ref`                      | `CodeRef` format and `validate_refs()` |
| 8 | `spec-api/slug`                          | Slug rules — document the `_`/`-` constraint |

Per spec:
- Replace `body.md` with hand-written content (Purpose / Key Types / Invariants / Pitfalls).
- Add `sections/design.md` (rationale + alternatives considered).
- Add `sections/acceptance.md` (observable acceptance criteria).
- Replace coarse impl-block CodeRefs with per-method refs.
- Validate: `spec refs <slug> validate --workspace-root .` → `valid: true`.
- Advance: `--state reviewed` → `--state approved`.
- If code is merged: `--state implemented`.

### Phase 4 — Hand-curate bodies (Priority B: 7 specs)

| #  | Slug                                       | Notes |
|----|--------------------------------------------|-------|
|  9 | `spec-cli`                                 | Subcommand catalog, JSON shape |
| 10 | `spec-cli/cli/commands/bootstrap`          | Algorithm details, slug-normalization rule |
| 11 | `spec-mcp`                                 | Tool catalog, contract stability |
| 12 | `spec-mcp/server`                          | Tool definitions and registration |
| 13 | `spec-http`                                | REST surface, route ordering quirk |
| 14 | `spec-http/routes`                         | Endpoint table |
| 15 | `spec-http/handlers/specs`                 | Read/write split |

Per spec: rewrite `body.md`, add `acceptance.md`, advance `reviewed → approved`.
`design.md` not required for Priority B.

### Phase 5 — Health gate

- `spec health --all --json` → `issues_count == 0`.
- `spec refs <slug> validate --workspace-root .` → `valid: true` for every
  Priority A + B spec.
- `spec tree` shows the curated hierarchy under `spec-system`.

## Resolved Risks

1. **No edge support in `spec-api`.** Confirmed: `crates/spec-api/src/`
   contains no edge management code (verified by source grep on
   `edge|parent_of|EdgeRecord|insert_edge|link`). The schema declares
   `parent_of`/`linked`/`depends_on` edge rules, but `SpecStore` exposes
   nothing to create or query them. `spec tree` walks the manifest `parent`
   string, not real edges.

   **Decision:** Phase 4 (cross-component `linked` edges) is removed from
   this ticket. A follow-up ticket *"Add edge management to spec-api
   (parent_of, linked, depends_on)"* must be created. Until that lands, parent
   relationships use the manifest `parent` string field as is.

2. **No undo on spec updates.** Mitigation: write each body in
   `humans/tmp/spec-bodies/<slug>.md` first, review locally, then
   `spec update <id> --body-file <path>`.

3. **Bootstrap-generated impl-block CodeRefs are too coarse** (e.g.
   `SpecStore` impl block at lines 32–480). Phase 3 explicitly replaces these
   with per-method refs for Priority A specs. Priority B + leaf specs
   tolerated as-is.

## Acceptance Criteria

- [x] Phase 1 bootstrap complete — 38 specs created, health clean.
- [ ] `spec-system` umbrella root spec exists with architecture diagram in `body.md`.
- [ ] Five crate-root specs (`memory-api`, `spec-api`, `spec-cli`, `spec-mcp`,
      `spec-http`) have `parent=<spec-system-uuid>`.
- [ ] All 8 Priority A specs have hand-curated `body.md`, `sections/design.md`,
      and `sections/acceptance.md`.
- [ ] All 7 Priority B specs have hand-curated `body.md` and `sections/acceptance.md`.
- [ ] All Priority A specs have per-method CodeRefs (no single ref spanning
      >100 lines) and pass `spec refs <slug> validate --workspace-root .`.
- [ ] All Priority A + B specs are at state `approved` or `implemented`.
- [ ] `spec health --all --json` reports `issues_count == 0`.
- [ ] Follow-up ticket created for "Add edge management to spec-api".

## Verification Commands

```bash
# Full health check
./target/debug/spec.exe health --all --json | jq '{specs_checked, issues_count}'

# Validate refs for a curated spec
./target/debug/spec.exe refs spec-api/store validate --workspace-root . --json

# Confirm hierarchy under the umbrella
./target/debug/spec.exe tree spec-system --json

# Confirm state advancement
./target/debug/spec.exe list --where state=approved --json | jq '.count'
```
# Bootstrap: Spec Files for the Spec System

## Objective

Write the initial specification files for the spec system itself, establishing the self-documenting foundation. These specs serve as both documentation and as test fixtures for the spec tooling.

## Specs to Create

### Root Spec: `spec-system` (scope: domain)
- Overview of the entire specification system
- Architecture diagram: memory-api → spec-api → CLI/MCP/HTTP
- Design decisions and rationale

### Module Specs

| Slug | Scope | Component | Code Target |
|------|-------|-----------|-------------|
| `memory-api` | crate | memory-api | `crates/memory-api/` |
| `memory-api/entity` | module | memory-api | EntityManifest, EntityStore |
| `memory-api/schema` | module | memory-api | TypeSchema, SchemaRegistry |
| `memory-api/edge` | module | memory-api | EdgeRecord, EdgeRegistry |
| `memory-api/query` | module | memory-api | Expr, parse_query |
| `memory-api/storage` | module | memory-api | EntityFs, RedbIndexStore, TantivySearchIndex |
| `spec-api` | crate | spec-api | `crates/spec-api/` |
| `spec-api/manifest` | module | spec-api | SpecManifest, CodeRef, FeatureStatus |
| `spec-api/store` | module | spec-api | SpecStore |
| `spec-api/schema` | module | spec-api | specification.toml state machine |
| `spec-api/skill-gen` | module | spec-api | Skill generation engine |
| `spec-cli` | crate | spec-cli | spec binary |
| `spec-mcp` | crate | spec-mcp | MCP tool surface |
| `spec-http` | crate | spec-http | HTTP endpoints |

## Acceptance Criteria

- [ ] All specs above created in the spec store
- [ ] Parent-child hierarchy correctly wired
- [ ] Code refs point to actual source files
- [ ] Body content describes current implementation state
- [ ] Specs pass `spec health` validation