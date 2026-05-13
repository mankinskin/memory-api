# Phase 1 — Core Backend + Search

**Status:** READY (Phase 0 formally closed)

Global progress tracking: `../EXECUTION_CHECKLIST.md`.
Checkboxes in this file are phase-scope deliverable gates.

## Objective

Implement a working distributed ticket store with integrated full-text search:
create, read, update, delete tickets with dependency edges, using redb as the
metadata index, Tantivy as the search index, and the filesystem as the artifact
store. All writes must be crash-safe. The FS watcher must be live so
discovered/orphaned tickets are integrated automatically. Search is wired from
day one — not deferred.

## Problem/Solution/Reference Baseline

1. Problem: multiple agents can race and create inconsistent local state.
Solution: strict per-ticket locks + serialized index mutations + idempotent reconcile.
Reference: concurrency goals inspired by `delightful-ai/beads-rs`.

2. Problem: operators and agents need predictable machine output for orchestration.
Solution: human CLI remains ergonomic, but agents talk directly in `TaskCommand` JSON through `ticket exec` from day one.
Reference: JSON-first command patterns in `Dicklesworthstone/beads_rust`, adapted to separate human UX from machine protocol.

3. Problem: swarm agents need one query surface for both planning text and operational filters.
Solution: unified query language combining FTS + structured predicates, wired into the write path.
Reference: machine-oriented list/ready/search workflow patterns in both beads projects.

## Deliverables

### Core CRUD
- [ ] `TicketFs::create(manifest, type_schema)` — atomic FS folder + redb index write
- [ ] `TicketFs::get(id)` — read manifest; validate against registered type schema
- [ ] `TicketFs::update(id, patch)` — validate state transition, atomic write, git commit
- [ ] `TicketFs::delete(id)` — soft-delete flag + remove from index
- [ ] `RedbIndexStore::add_edge(from, to, kind: String)` — open edge kind + cycle check
- [ ] `RedbIndexStore::list(filter)` — scan index with metadata predicates
- [ ] Per-ticket lock: `.ticket-lock` acquired before write, released after commit
- [ ] Short-lived global index lock for index row insertions/removals

### Watcher + Reconcile
- [ ] `FsWatcher` (notify): watches registered scan roots; on CREATED/MODIFIED triggers
      reconcile; on MOVED updates index path; on DELETED marks orphan
- [ ] `Reconciler::integrate_orphan(path)` — parse + validate; add to index or emit
      `ParseError` diagnostic

### Search (merged from old Phase 3)
- [ ] `TantivySearchIndex::upsert(id, doc)` — index new/updated ticket
- [ ] `TantivySearchIndex::remove(id)` — delete from index on ticket deletion
- [ ] `QueryParser::parse(expr: &str) -> Query` — unified AST from query string
- [ ] `TantivySearchIndex::search(query, limit, highlight) -> Vec<SearchResult>` —
      returns ranked results with highlighted snippet text
- [ ] `ticket scan --reindex` flag — full Tantivy + redb rebuild from FS content
- [ ] Binary content extractor registry: text → pass-through; PDF → best-effort text
      extract; unknown binary → filename + metadata only

### CLI Commands (MVP)
- [ ] `ticket create`, `ticket get`, `ticket update`, `ticket list`,
      `ticket delete`, `ticket scan`, `ticket search`

### Agent Protocol (MVP)
- [ ] `ticket exec` — read one `TaskCommand` JSON request from stdin and return one structured result envelope
- [ ] `ticket exec --batch` — read multiple commands from stdin and execute as one transaction
- [ ] Explicit `index_root` required for `ticket exec` requests
- [ ] Full UUIDs required for agent protocol requests (no prefix matching)
- [ ] Structured `patch` objects for updates (no `--field k=v` parsing in machine mode)
- [ ] Optional `fields` selector on read/search/update responses to reduce token output

## Atomic Write Protocol

```
1. Acquire per-ticket lock (.ticket-lock via fs2)
2. Write ticket.toml + content files to temp folder (<uuid>.tmp/)
3. Begin redb write transaction (index lock acquired implicitly)
4. Rename temp folder → final UUID folder (atomic POSIX; best-effort Windows)
5. Insert/update redb index row
6. Commit redb transaction
7. git commit the changed files via git2 (history write; non-blocking on failure)
8. Release per-ticket lock
```

On crash between steps 4 and 5: `.tmp` folder present, no index row → `ticket scan`
detects and integrates or reports error.

## redb Tables (draft, finalised in Phase 0)

```rust
const TICKETS: TableDefinition<&str, &[u8]> = TableDefinition::new("tickets");
// key: uuid string, value: bincode(IndexedTicket { path, manifest_fields, type_id, ... })

const EDGES: TableDefinition<(&str, &str, &str), ()> = TableDefinition::new("edges");
// key: (from_uuid, to_uuid, kind_str), value: ()

const SCAN_ROOTS: TableDefinition<&str, &str> = TableDefinition::new("scan_roots");
// key: absolute path, value: registered label

const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
// schema_version, index_root, git_repo_path, ...

const LEASES: TableDefinition<&str, &[u8]> = TableDefinition::new("leases");
// key: uuid string, value: bincode(LeaseInfo { working_by, lease_expires_at, work_intent })
```

## Cycle Detection

On `add_edge(A → B, kind)`: BFS/DFS from B; if A is reachable, reject with
`DependencyCycle` error. Run only for directed dependency-type edges; the type
definition declares whether an edge kind is acyclic-enforced.

## Key Interview Answers Applied Here

| Answer | Backend impact |
|--------|---------------|
| Q1 — Distributed FS | No single tickets/ root; index maps UUID → absolute path |
| Q2 — UUID | Folder name = UUID string; no sequential counter |
| Q4 — Open edge kinds | Edge table key includes kind as plain string |
| Q6 — Per-ticket lock | `.ticket-lock` per folder; short global lock for index ops |
| Q8 — Any attachments | `assets/` subdirectory created; index stores file list |
| Q10 — FS tracking | `FsWatcher` + `Reconciler` are Phase 1 deliverables, not deferred |

## Additional Swarm Deliverables

Lease primitives are specified and implemented in Phase 1.5 (see `015_phase_lease_protocol/PLAN.md`).
The `LEASES` redb table is created in this phase but write/read logic is Phase 1.5 scope.

## Default Schema — tracker-improvement

Phase 1 ships one hardcoded ticket type: `tracker-improvement`.
The schema engine traits exist but only this built-in type is supported initially.

Fields:
- `id` (UUID, required, universal)
- `created_at` (ISO 8601, required, universal)
- `title` (string, required)
- `type` (string, required — always "tracker-improvement" for now)
- `state` (enum: open, in-progress, review, validating, validated, release-candidate, released, monitoring, done, blocked, cancelled)
- `component` (string, optional)
- `risk_level` (enum: low, medium, high)
- `acceptance_criteria` (string, optional)
- `validation_plan` (string, optional)
- `validation_status` (enum: pending, in-progress, passed, failed)
- `validator_id` (string, optional)
- `evidence_refs` (list of strings, optional)
- `release_target` (string, optional)
- `release_version` (string, optional)
- `bug_links` (list of UUIDs, optional)
- `bootstrap_blocker` (bool, optional)
- `rollout_stage` (enum: mirror, hybrid, tracker-first)
- `blocked_by` (list of UUIDs, optional — field hint until edge commands are wired)

State transitions (default):
- open → in-progress, blocked, cancelled
- in-progress → review, blocked, cancelled
- review → validating, in-progress, blocked
- validating → validated, review, blocked
- validated → release-candidate, review, blocked
- release-candidate → released, review, blocked
- released → monitoring, blocked
- monitoring → done, blocked
- blocked → open, in-progress, review, cancelled
- done → (terminal, reopenable via explicit command)
- cancelled → (terminal, reopenable via explicit command)

Extension: additional fields passed as key-value pairs are stored in `extra` map
and indexed by Tantivy as `x_<field_name>` text fields.

Validation and release semantics are defined centrally in `VALIDATION_RELEASE_GOVERNANCE.md`.

## Unified Query Language

Grammar (informal):

```
query  := expr (WS expr)*
expr   := field_pred | fts_term | quoted_phrase
field_pred := IDENT ":" value
value  := bare_word | quoted_string | range
range  := "[" value "TO" value "]"
```

Examples:
```
"login page"                                          # free-text phrase
status:open                                           # exact field match
assigned:alice "login page"                           # combined
status:open created:[2026-01-01 TO 2026-03-31]        # date range + status filter
```

Field names map to Tantivy `FAST`/`STRING` fields; unrecognised field names surface
as a query parse error with a suggestion.

## Tantivy Field Schema

```rust
// Universal fields (always present)
schema_builder.add_text_field("id",          STRING | STORED);
schema_builder.add_date_field("created_at",  INDEXED | STORED | FAST);
schema_builder.add_date_field("updated_at",  INDEXED | STORED | FAST);
schema_builder.add_text_field("ticket_type", STRING | STORED | FAST);

// Well-known optional fields (populated when present in manifest)
schema_builder.add_text_field("body",        TEXT | STORED);  // description.md content
schema_builder.add_text_field("attachments", TEXT | STORED);  // extracted text from assets

// Dynamic fields from type schema registered when a type schema is loaded
// (e.g. status, assignee, priority, labels)
// Namespace prefix x_ reserved to avoid collision with universal fields
```

## Index Lifecycle

- **Write path**: `create`/`update` calls `TantivySearchIndex::upsert` before
  releasing the per-ticket lock.
- **Crash recovery**: if the process dies after the redb commit but before the Tantivy
  write, `ticket scan --reindex` rebuilds the full index. Tantivy is always derived;
  never source-of-truth.
- **Schema evolution**: index schema version stored in redb `META`; schema change
  triggers automatic full reindex on next startup.

## Read Consistency Model

- **Write path (strongly consistent):** Every `create`/`update`/`delete` operation
  commits to both the filesystem and the redb index synchronously within the per-ticket
  lock. The Tantivy index is also updated before lock release. A successful command
  return guarantees the ticket is immediately queryable.
- **External edits (eventually consistent):** Changes made outside the `ticket` CLI
  (manual file edits, folder moves, copies) are detected by the FS watcher and
  reconciled asynchronously. SLO target: external changes visible within 10 seconds
  under normal load.
- **Correctness repair:** `ticket scan --reindex` performs a full filesystem walk and
  rebuilds both the redb index and the Tantivy index from scratch. This is the
  canonical recovery mechanism after crashes, missed watcher events, or suspected
  index corruption.

## Staged Command Rollout

### MVP (this phase)
Human: create, get, update, list, delete, scan, search

Agent: `ticket exec` for the same `TaskCommand` set, plus transactional batch mode

### Phase 1.5
claim, unclaim, heartbeat, leases, `ticket serve --stdio`

### Phase 2
history, diff, revert, finalize-merge

### Phase 3
deps, blocked-by, blocking, critical-path, validate-graph, export-graph, board, merge-queue

## Hosting Strategy

The command contract is transport-first, not CLI-first.

### Human surface

Phase 1 ships `ticket` CLI subcommands for operators:

- cwd-based `index_root` inference is allowed
- short UUID prefixes are allowed where unambiguous
- short flags and terminal formatting are allowed

### Agent surface

Phase 1 ships `ticket exec` as the primary machine interface:

- request body is `TaskCommand` JSON from stdin
- every request must include explicit `index_root`
- full UUIDs only
- responses are structured JSON envelopes with optional field projection

### Persistent machine surface

Phase 1.5 adds `ticket serve --stdio`:

- JSONL request/response over one long-lived process
- redb and Tantivy stay open for session lifetime
- lease renewal may be tied to session liveness

### Adapters

Phase 5 exposes the same `TaskCommand` contract through `context-http` and `context-mcp`.
CLI, stdio, HTTP, and MCP are all adapters over one command model.

No daemon is required in Phase 1. `ticket exec` remains stateless. `ticket serve --stdio`
is introduced only when persistent lease-heavy workflows justify it.

## Risks

- Windows does not guarantee atomic folder rename; document fallback behaviour.
- FS watcher events can fire multiple times for a single user operation (debounce needed).
- `notify` crate backend varies by OS (inotify / FSEvents / ReadDirectoryChangesW);
  test on all three.
- Tantivy index directory must be excluded from OS file-sync tools (OneDrive, Dropbox)
  to avoid corruption; document this prominently.
- Index schema changes require full re-index; version the schema in `META`.
- Dynamic type-schema fields may collide with universal field names; enforce namespace
  prefix `x_<type>_<field>`.

## TODO

- TODO: Write crash-safety integration test (kill process mid-write, verify `ticket scan` recovers).
- TODO: Define debounce window for watcher events (suggested: 200 ms).
- TODO: Confirm list filter set with first workflow definition.
- TODO: Decide index storage path: `.context-engine/ticket-index/search/` alongside redb.
- TODO: Write query language grammar as a formal PEG/pest grammar before implementation.
- TODO: Benchmark index rebuild time for 10 000 tickets with mixed binary attachments.
- TODO: Define `ticket exec --batch` transactional envelope and rollback boundary.
- TODO: Define exact `fields` projection semantics for search result snippets vs full ticket payloads.
