# Phase 2 — `presentation-api` + `presentation` facade crate + `.presentation` store schema

Parent epic: `0ee95228` — [presentation] Epic: script-to-deck presentation system.
Governing spec: `2ccde9ee` — Presentation System.

This ticket is specified to implementation depth. Follow it literally; escalate rather
than improvising if a step contradicts the codebase.

---

## 0. Prerequisites and reference reading

Read before writing code (bounded reads only):

1. `WORKFLOW_TOOLS_DOMAIN_CRATE_CONTRACT.md` — the normative crate/binary/feature layout.
2. `workflow-tools-contract-reference/` — the compiling reference implementation
   (`example-api` internal crate + `example` facade + gated cli/mcp/http bins).
3. `memory-api/crates/spec-api/src/` — the closest existing analogue. Specifically
   `manifest.rs`, `store.rs`, `store_index.rs`, `slug.rs`, `workspace.rs`, `error.rs`,
   and `store/{helpers,hierarchy,sections}.rs`. The deck model is deliberately shaped
   like the spec model (`spec.toml` + `body.md` -> `deck.toml` + `slides.md`).
4. `memory-api/crates/ticket/src/{lib.rs,bin/}` — the facade + gated-binary pattern in
   production use.
5. `memory-kernel/crates/transport-harness` — shared CLI/MCP/HTTP scaffolding. Do not
   re-implement argument parsing, MCP server bootstrap, or HTTP routing scaffolding.

Do **not** copy spec-api wholesale. Reuse its structural conventions, not its domain
semantics.

---

## 1. Repository layout to create

```text
memory-viewers/presentation-viewer/
  crates/
    presentation-api/          # internal domain API
      Cargo.toml
      src/
        lib.rs
        error.rs
        slug.rs
        workspace.rs
        manifest.rs            # DeckManifest, SlideRef, ThemeRef, TraceLinks
        store.rs               # PresentationStore: discovery, load, save
        store/
          helpers.rs
          slides.rs            # slide-level CRUD + reorder
          materialize.rs       # deck entity -> Slidev sources on disk
          tests.rs
        theme.rs               # theme/preset registry + override resolution
        build.rs               # slidev dev/build orchestration + artifact tracking
        trace.rs               # links to specs/tickets a deck presents
      tests/
    presentation/              # public facade + transport binaries
      Cargo.toml
      src/
        lib.rs                 # re-exports presentation-api public surface
        bin/
          presentation-cli.rs
          presentation-mcp.rs
          presentation-http.rs
```

The Cargo workspace root that owns these crates is the same one that already owns
`memory-viewers/spec-viewer`. Add both crates to its members list.

### 1.1 `presentation` facade `Cargo.toml`

```toml
[lib]
name = "presentation"
path = "src/lib.rs"

[features]
default = []
cli  = ["dep:transport-harness"]
mcp  = ["dep:transport-harness"]
http = ["dep:transport-harness"]

[[bin]]
name = "presentation-cli"
path = "src/bin/presentation-cli.rs"
required-features = ["cli"]

[[bin]]
name = "presentation-mcp"
path = "src/bin/presentation-mcp.rs"
required-features = ["mcp"]

[[bin]]
name = "presentation-http"
path = "src/bin/presentation-http.rs"
required-features = ["http"]
```

`src/lib.rs` is `pub use presentation_api::*;` plus transport-agnostic composition
helpers only. No transport code in the library.

**Decided:** `presentation-http` ships a **full CRUD surface in v1**. See §4.3.

---

## 2. Store schema — `.presentation/` at repo root

Mirror the `.spec/` and `.ticket/` conventions.

```text
.presentation/
  decks/
    <deck-uuid>/
      deck.toml
      slides.md
      components/            # agent-authored .vue/.ts components for this deck
      assets/                # images, data files
      theme.override.toml    # optional per-deck theme override
      dist/                  # build artifact output (gitignored)
  themes/
    <theme-name>/            # repo theme pack(s) — populated by ticket 60222b57
  index/                     # generated store index, mirrors spec-api's store_index
```

### 2.1 `deck.toml` schema

```toml
id          = "uuid-v4"
slug        = "workflow-tools-intro"   # hierarchical, kebab, unique in store
title       = "Workflow Tools: Suite Introduction"
component   = "workflow-tools"          # owning component, like spec.component
state       = "draft"                   # draft | in-review | published | archived
created_at  = "RFC3339"
updated_at  = "RFC3339"

theme       = "seriph"                  # stock theme name in Phase 1; repo pack later
aspect      = "16:9"
presenter_notes = true

[[slides]]
id      = "uuid-v4"
order   = 1
title   = "What are workflow tools?"
layout  = "cover"                       # must resolve in the preset registry
preset  = "hero-dark-gradient"          # optional preset refinement
anchor  = "hero"                        # stable in-deck anchor / route fragment

[trace]
specs   = ["2ccde9ee-85ac-4c87-9601-f6099f5be01c"]
tickets = ["0ee95228-475d-4706-a108-fd208f7c4098"]
```

Rules:
- `id` is authoritative; `slug` is the human handle and must be unique per store.
- `slides[].order` is dense and 1-based; reorder operations renormalize.
- `slides[].id` is stable across reorder — it is the addressable slide entity id.
- Unknown top-level keys are preserved on round-trip (same policy spec-api uses for
  its manifest; verify the exact mechanism in `spec-api/src/manifest.rs` and match it).

### 2.2 `slides.md` schema

Standard Slidev markdown. Slides separated by `---`. Each slide's frontmatter carries
`layout:` and a `deckSlideId:` key binding it back to `deck.toml`'s `slides[].id`.
`slides.md` is the source of truth for slide **content**; `deck.toml` is the source of
truth for slide **identity, order, and metadata**. Materialization reconciles the two;
a mismatch (orphan `deckSlideId`, or a manifest slide with no body) is a validation error.

---

## 3. `presentation-api` responsibilities

Implement all six, in this order.

### 3.1 Deck registry CRUD
- `PresentationStore::discover(workspace) -> Store` — walk up for `.presentation/`,
  matching how `spec-api/src/workspace.rs` resolves store roots. Support nested-root
  aggregation for reads if spec-api does; if unsure, match spec-api's behavior exactly.
- `create_deck(title, slug, component, theme, body: Option<String>) -> DeckId`
- `get_deck(id_or_slug) -> Deck`
- `list_decks(where_clauses) -> Vec<DeckSummary>`
- `update_deck(id, fields)` — title, state, theme, aspect, trace links.
- `delete_deck(id)` — refuse if `dist/` is being served; require explicit force.
- Slug resolution and prefix-id resolution must behave like spec-api's
  (`slug.rs` + id-prefix matching). Reuse the algorithm; do not invent a new one.

### 3.2 Slide-level CRUD
- `add_slide(deck, at: Option<usize>, title, layout, preset, content) -> SlideId`
- `update_slide(deck, slide, {title, layout, preset, content})`
- `reorder_slides(deck, slide, to_index)` — renormalize `order` densely.
- `delete_slide(deck, slide)` — removes both the manifest entry and the `slides.md` block.
- Every mutation rewrites `deck.toml` and `slides.md` atomically together (write to temp,
  fsync, rename). A partial write that desynchronizes manifest and body is a defect.

### 3.3 Materialization — deck entity -> Slidev sources
`materialize(deck, out_dir)` produces a Slidev-buildable directory:
- `slides.md` with resolved theme/layout frontmatter injected per slide.
- `components/` copied from the deck plus any theme-pack components.
- `assets/` copied.
- `package.json` / Slidev config emitted from the theme pack (ticket `89b0c64a` defines
  these files for the stock theme; this phase only wires them up — do not redesign them).
Materialization is **pure and idempotent**: same deck in, byte-identical output.

### 3.4 Build orchestration
- `build(deck, mode: Dev | Static)` — shells out to the Slidev toolchain established in
  ticket `89b0c64a`. `Dev` starts the hot-reload server and returns a handle + port;
  `Static` produces `dist/` and returns an
  `Artifact { path, hash, built_at, deck_revision }`.
- Track artifacts in the store index so the viewer can tell stale from fresh.
- Never assume Node is present: probe and return a typed `ToolchainMissing` error with
  the exact remediation command.

### 3.5 Theme/preset registry
- Load theme packs from `.presentation/themes/`.
- Resolve effective theme = theme pack <- `theme.override.toml` <- per-slide `preset`.
- `list_presets(theme) -> Vec<PresetDescriptor>` with name, description, required slots.
  This is what the authoring agent reads to pick layouts, so descriptors must be
  self-describing enough to choose from without reading theme source.
- In Phase 1 the only theme is a **stock Slidev theme**, so the registry must degrade
  gracefully: a theme with no repo-authored descriptors falls back to the stock theme's
  built-in layout names. Ticket `60222b57` supplies the first real descriptor set; this
  registry is the contract it targets.
- Unknown layout/preset in `deck.toml` is a hard validation error naming the valid set.

### 3.6 Traceability
- `[trace]` links decks to spec ids and ticket ids.
- `validate_trace(deck)` verifies each referenced id resolves in the corresponding store,
  following the pattern in `spec-api/src/ticket_ref.rs`.
- Dangling references are warnings, not errors, unless the deck state is `published`.

---

## 4. Transport surfaces

### 4.1 `presentation-cli`
```
presentation deck create --title <s> --slug <s> --component <c> --workspace <path>
presentation deck list [--where k=v] [--toon|--json]
presentation deck get <id> [--full]
presentation deck update <id> [--title|--state|--theme|--aspect]
presentation deck delete <id> [--force]
presentation slide add <deck> [--at N] --title <s> --layout <l>
presentation slide update <deck> <slide> ...
presentation slide move <deck> <slide> --to N
presentation slide delete <deck> <slide>
presentation theme list [--deck <id>]
presentation preset list [--theme <t>]
presentation materialize <deck> --out <dir>
presentation build <deck> [--dev|--static]
presentation validate <deck>
```
Support `--toon` on every read command (repo compact-output convention) and default to
TOON over JSON for machine-readable output.

### 4.2 `presentation-mcp`
Tools, one per CLI verb, named `presentation_*`:
`presentation_create_deck`, `presentation_list_decks`, `presentation_get_deck`,
`presentation_update_deck`, `presentation_delete_deck`, `presentation_add_slide`,
`presentation_update_slide`, `presentation_move_slide`, `presentation_delete_slide`,
`presentation_list_themes`, `presentation_list_presets`, `presentation_materialize`,
`presentation_build`, `presentation_validate`.
Every tool takes `workspace` and `caller_model` to match the other domain MCP servers.
Register the server in the repo MCP config alongside `spec-mcp` and `ticket-mcp`.

### 4.3 `presentation-http` — full CRUD in v1

```
GET    /decks                          # list, supports ?where= filters
POST   /decks                          # create
GET    /decks/{id}                     # get
PATCH  /decks/{id}                     # update fields/state/theme/aspect
DELETE /decks/{id}                     # delete (?force=true)

GET    /decks/{id}/slides              # list slides in order
POST   /decks/{id}/slides              # add (?at=N)
GET    /decks/{id}/slides/{slide}      # get
PATCH  /decks/{id}/slides/{slide}      # update title/layout/preset/content
POST   /decks/{id}/slides/{slide}/move # reorder, body {to: N}
DELETE /decks/{id}/slides/{slide}      # delete

GET    /themes                         # list themes
GET    /themes/{name}/presets          # preset descriptors
POST   /decks/{id}/materialize         # body {out}
POST   /decks/{id}/build               # body {mode: "dev"|"static"} -> Artifact
GET    /decks/{id}/validate            # validation report
```

Route wiring uses `transport-harness`. Map `PresentationError` variants to status codes
consistently with the other domain HTTP servers: `*NotFound` -> 404, `SlugConflict` -> 409,
`InvalidSlug` / `Unknown*` / `ManifestBodyDesync` -> 400, `ToolchainMissing` /
`BuildFailed` -> 503, `Io` -> 500. Every 4xx body names the valid alternatives where the
error carries them.

---

## 5. Error model

One `PresentationError` enum in `error.rs`, modeled on `spec-api/src/error.rs`:
`StoreNotFound`, `DeckNotFound`, `SlideNotFound`, `SlugConflict`, `InvalidSlug`,
`ManifestParse`, `BodyParse`, `ManifestBodyDesync`, `UnknownLayout`, `UnknownPreset`,
`ToolchainMissing`, `BuildFailed`, `Io`. Each variant carries enough context to act on
(ids, the valid set for unknown-layout, the remediation command for toolchain-missing).

---

## 6. Testing

- Unit tests colocated per module, following `spec-api/src/store/tests.rs`.
- Integration tests in `crates/presentation-api/tests/` over a temp `.presentation/` store.
- Required cases:
  - create/get/list/update/delete deck round-trip, including unknown-key preservation.
  - slug conflict and id-prefix resolution.
  - add/reorder/delete slide keeps `deck.toml` and `slides.md` in sync; order stays dense;
    slide ids stay stable across reorder.
  - atomic-write failure injection leaves the store consistent (no desync).
  - materialization is byte-identical across two runs.
  - unknown layout/preset produces `UnknownLayout`/`UnknownPreset` naming the valid set.
  - theme registry falls back to stock-theme layout names when no descriptors exist.
  - manifest/body desync is detected by `validate`.
  - `ToolchainMissing` is returned (not a panic) when Node/Slidev is absent.
  - trace validation: resolvable ids pass; dangling ids warn in `draft`, fail in `published`.
- HTTP: one integration test per route group asserting status-code mapping.
- Use `init_test_tracing!` for tracing-based tests; read failures from `target/test-logs/`.
- Record validation evidence via test-api (`test_record_spec` / `test_record_execution`)
  and link it to this ticket id.

---

## 7. Definition of done

- `cargo build -p presentation` builds a slim library with no transport deps.
- `cargo build -p presentation --features cli,mcp,http` builds all three binaries.
- `cargo test -p presentation-api` passes.
- `cargo clippy -p presentation-api -p presentation -- -D warnings` is clean.
- A deck created via `presentation-cli` is readable via `presentation-mcp` and over
  `presentation-http`, and vice versa.
- `.presentation/` is created lazily on first write and `.presentation/decks/*/dist/` is
  gitignored.
- Spec `2ccde9ee` linked to this ticket.
- Ticket moved to `in-review` only after the above, with evidence linked.

## 8. Explicit non-goals for this ticket

- No Dioxus viewer work (ticket `345528ff`).
- No theme-pack authoring (ticket `60222b57`); this ticket only builds the registry that
  consumes it.
- No skills or agent-mode work.
- No PDF export.
