# [ticket-api] Cross-workspace move + automatic reference re-linking for store entries

## Goal

Provide a first-class, safe operation to **move a store entry (primarily a ticket; ideally any memory-api entry) from one workspace/store to another** and **automatically re-link every reference to and from it** so no dangling edges, orphaned board entries, or stale index rows remain in either store.

This is a **planning ticket** — it owns the design and hand-off. Implementation happens in follow-on tickets created from the plan below.

## Problem / current state

There is **no move/relocate/transfer capability** today. Confirmed by codebase research (no `move`/`relocate`/`transfer`/`migrate`/`change_workspace` command, route, or MCP tool in `ticket-api`, `ticket-cli`, `ticket-http`, or `ticket-mcp`).

A workspace is **not a field** on a ticket — it is **inferred from the filesystem path** to the nearest `.ticket/` store. Each store has its own SQLite index (`tickets.db`) and Tantivy search index (`search_index/`). Moving an entry therefore means physically relocating its folder into a different `.ticket/` store and reconciling **two separate indexes**, not flipping a field.

Concrete trigger: ticket `694d74b4` ("Integrate Rust/WASM core into TS hosts…") was created in the **root** store (`context-engine/.ticket/`) but belongs in the **memory-api** store (`memory-viewers/memory-api/.ticket/`) alongside the rest of its track. A reminder ticket tracks moving it once this tool exists.

## Research findings (frozen context for the implementer)

File:line references are from the research pass on `memory-viewers/memory-api`.

### Workspace + storage model
- Workspace is inferred from path; no `workspace` field in the manifest. `IndexedEntity` (`crates/memory-api/src/storage/indexed.rs`) carries `id`, `path`, `type_id`, `title`, `state`, timestamps — no workspace.
- Workspace discovery walks up to the nearest `.ticket/` (`crates/memory-api/src/workspace.rs` ~L154-184); nested scan roots are discovered within one root (~L184-227).
- `WorkspaceList/New/Use/Current/Remove` enum variants exist (`crates/ticket-api/src/contracts/command_schema.rs` L51-56) but are effectively **deprecated/unwired** — do not build on them without re-validating.
- On disk per store: `.ticket/{tickets.db, search_index/, tickets/<uuid>/{ticket.toml, history.ndjson, assets/, .ticket-lock}}`.
- Ticket folders are named by UUID; `TicketFs::create()` (`crates/ticket-api/src/storage/ticket_fs.rs` ~L83-140) writes atomically via `.tmp/` + rename.

### Edges / references
- Edges are **dual-stored**: file-backed in `ticket.toml` `[extra]` as `depends_on = [...]` / `linked = [...]`, AND cached in the SQLite `EDGES` table (from_id, to_id, kind). Managed by `add_edge()` / `remove_edge()` (`crates/ticket-api/src/storage/store/query.rs` ~L91-164).
- Edges are keyed by **UUID pairs**. They only resolve within a single aggregate index. A parent store that aggregates nested `.ticket/` scan roots can hold cross-nested edges; two **sibling** stores cannot see each other's UUIDs.
- `scan(reindex)` (`crates/ticket-api/src/storage/store/scan.rs` ~L55-104): with `reindex=true` it clears the SQLite edge table + Tantivy index and backfills from manifests (~L81).

### Cross-store references that go stale on a move
- **Board/draftboard**: `BoardEntry` and `LeaseInfo` reference the ticket UUID and live in the source store's SQLite (`crates/ticket-api/src/storage/store/board.rs` ~L29-92). After a move they become **orphaned** in the old store and **absent** in the new store.
- **Inbound edges**: any other ticket whose `depends_on`/`linked` points at the moved UUID. If the linker lives in the old store and the target moves to a different store, the edge becomes **dangling**.
- **Specs**: link tickets **textually** in markdown bodies (no schema-enforced array; `spec-api` `CodeRef` only holds `file_path`+`symbol`). Spec bodies that cite a ticket **folder path** go stale; bodies that cite only the UUID stay valid but should still be reviewed.
- **Tests/validation**: executions link tickets via `ticket_ids`; these are UUID-keyed and survive a move, but any stored **path** goes stale.

### ID stability
- Ticket UUIDs are **stable** across a move (folder named by UUID; UUID generated at create and never changes — `crates/ticket-api/src/storage/store.rs` ~L306-309). Re-linking can therefore key on UUID; only **paths** and **per-store index rows** must be rewritten.

### Hardest part
- **Dual-index consistency across two stores with no atomic multi-store transaction.** A correct move must coordinate: physical folder relocation, source-store edge/board/search/sqlite removal, target-store insertion, and rewriting of inbound linkers' manifests — with a failure window at every step. Needs a **move journal / resumable + rollbackable** design, not a best-effort sequence.

## Scope of this planning ticket

Produce a complete, reviewed design and the follow-on implementation tickets. In scope to **decide**:

1. **Move semantics**: same-aggregate-index move (nested ↔ parent, cheap re-path) vs. cross-store move (separate sibling stores, full migration). Define both; they are different difficulty tiers.
2. **Reference graph to re-link**: outbound edges, inbound edges (requires reverse lookup of all linkers), board entries/leases, and a **path-rewrite pass** for specs/tests/docs that cite the old folder path.
3. **Atomicity strategy**: move journal with phases (validate → stage → relocate → reindex source → reindex target → rewrite linkers → commit/rollback), resumable after interruption.
4. **Surface design**: new CLI command (`ticket move <id> --to-workspace-root <path>` or `--to-workspace <name>`), MCP tool, and HTTP route. Identify the wiring points:
   - CLI: `tools/cli/ticket-cli/src/cli/dispatch.rs` (~L27-47) + new handler in `tools/cli/ticket-cli/src/cli/commands/`.
   - HTTP: `tools/http/ticket-http/src/serve/routes.rs` (~L65-101).
   - MCP: `tools/mcp/ticket-mcp/` tool registration.
5. **Generality**: decide whether v1 is ticket-only or generalizes to any memory-api entry (specs, etc.). Recommend ticket-only for v1 with a clear extension seam.
6. **Dangling-reference policy**: fail-closed (refuse move if inbound linkers can't be reached/rewritten) vs. warn-and-record. Define the safe default.

## Acceptance criteria (planning)

- [ ] A reviewed design doc/section captures move semantics for both same-index and cross-store cases, the full reference set to re-link, and the journal/rollback model.
- [ ] Follow-on implementation tickets are created and linked under an appropriate tracker (core move op, reference re-linking, board migration, CLI surface, MCP surface, HTTP surface, tests), with dependencies ordered.
- [ ] The design names the exact integration points (CLI dispatch, HTTP routes, MCP tools) and the storage methods that must change.
- [ ] The reminder ticket to move `694d74b4` into the memory-api store is linked as a downstream consumer of the delivered tool.
- [ ] Spec traceability: a spec for the move/relink contract is created or an existing one updated, with acceptance criteria a reviewer can verify.

## Non-goals

- Implementing the move operation (follow-on tickets).
- Multi-entry batch moves (can be a later enhancement once single-entry move is correct).
- Reviving the deprecated `Workspace*` command variants.

## Hand-off notes

- Validate the deprecated-status of the `Workspace*` enum before relying on or removing it.
- Prefer keying all re-linking on stable UUIDs; treat any stored **path** as the thing that goes stale.
- Cross-store moves have no atomic transaction — the journal/rollback design is the crux of correctness and must be reviewed before implementation starts.
