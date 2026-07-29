## Objective

Give a ticket directory a `parts/` folder holding one markdown file per typed content part, indexed by a `[[parts]]` table in `ticket.toml`, with a schema-validated core kind vocabulary and lossless passthrough for free-form kinds.

## Requirements

- Ticket directory gains `parts/`, one markdown file per part.
- `ticket.toml` gains a `[[parts]]` table; each entry carries a stable opaque `id` assigned at creation, `kind`, `path`, `frozen` (bool), `created_at`, and optional `supersedes`.
- `id` is the addressing key; manifest order remains creation/display order, but kind+index is never an addressing key.
- The manifest is the source of truth for which parts exist and in what order; an orphan file in `parts/` is reported, not silently adopted.
- Core kinds are schema-validated: `objective`, `requirements`, `design`, `examples`, `acceptance_criteria`, `review`, `validation`, `notes`, `amendment`.
- Kinds outside the core vocabulary are accepted and stored as opaque attachments: preserved, listed, and retrievable, but not interpreted by projections.
- Multiple parts of the same kind are allowed; ordering within a kind is stable because each part has a distinct `id`.
- Existing tickets without a `[[parts]]` table load unchanged; `description.md` is presented as a single implicit `objective` part.
- `supersedes` is reserved from the start, stores the `id` of the frozen part an `amendment` later supersedes, and stays unused until the freeze ticket lands.

## Design

The concrete manifest model remains `memory_api::model::entity::EntityManifest`, re-exported as `TicketManifest` from `memory-api/crates/ticket-api/src/model/ticket.rs`. `memory-api/crates/ticket-api/src/model/filesystem.rs` owns the TOML parse entry point (`parse_ticket_manifest_toml`), while `memory-api/crates/ticket-api/src/storage/ticket_fs.rs` owns the file-backed create/read/update/scan helpers that materialize the ticket folder on disk.

Core-kind validation belongs in `memory-api/crates/ticket-api/src/model/schema_registry.rs` through `SchemaRegistry::validate_manifest` and the loaded `TicketTypeSchema`, so the new `[[parts]]` table participates in the same manifest-validation path as `required_states`. The stable `parts[].id` field is the only write target for part-addressed updates; `kind` is descriptive metadata, not an address.

`TicketFs::create`, `TicketFs::read`, and `TicketFs::update` keep the on-disk manifest authoritative, and the compatibility surface for legacy tickets remains `description.md` until migration converts them.

## Implementation Steps

1. Extend `memory-api/crates/memory-api/src/model/entity.rs` and `memory-api/crates/ticket-api/src/model/ticket.rs` with a typed part-entry structure and a `parts` collection on `TicketManifest` that serializes the new manifest table.
2. Update `memory-api/crates/ticket-api/src/model/filesystem.rs` and `memory-api/crates/ticket-api/src/storage/ticket_fs.rs` to parse, format, and round-trip `[[parts]]` entries, including `id`, `kind`, `path`, `frozen`, `created_at`, and optional `supersedes`.
3. Teach `memory-api/crates/ticket-api/src/model/schema_registry.rs` to validate the core vocabulary and reject unknown core kinds with the valid set in the error.
4. Add stable opaque part-id assignment in `memory-api/crates/ticket-api/src/storage/store.rs` so newly created parts get a durable `id` before any write or migration path can target them.
5. Extend `memory-api/crates/ticket-api/src/storage/ticket_fs.rs::scan_root` and the related consistency checks so orphan `parts/` files are reported rather than adopted.
6. Preserve the legacy single-description fallback in `memory-api/crates/ticket-api/src/storage/store.rs` and `memory-api/crates/ticket-api/src/storage/ticket_fs.rs` so tickets without `[[parts]]` still read `description.md` as the implicit `objective` part.
7. Add manifest round-trip, duplicate-kind ordering, orphan-file, and legacy-load tests under `memory-api/crates/ticket-api/src/storage/tests/`.

## Examples

```toml
[[parts]]
id = "p_01J0Y2Q6XK9WZQ5H9K9C2N7M2R"
kind = "objective"
path = "parts/objective.md"
frozen = true
created_at = "2026-07-29T12:00:00Z"

[[parts]]
id = "p_01J0Y2Q9A0N2J7W5D3G8J1X7R2"
kind = "review"
path = "parts/review-001.md"
frozen = false
created_at = "2026-07-30T09:00:00Z"

[[parts]]
id = "p_01J0Y2QB7M8N4V6C1T2R9S3H4J"
kind = "benchmark-notes"
path = "parts/benchmark-notes.md"
frozen = false
created_at = "2026-07-30T10:00:00Z"
```

## Acceptance Criteria

1. Creating a part writes both the file under `parts/` and its `[[parts]]` manifest entry atomically; a crash between the two leaves no half-state.
2. A core kind that fails schema validation is rejected with the offending kind and the valid vocabulary in the error.
3. A free-form kind round-trips byte-identically through write, list, and read.
4. Two `review` parts coexist and list in stable creation order.
5. A ticket directory with no `[[parts]]` table loads, and its `description.md` reads back as an `objective` part.
6. A file present in `parts/` with no manifest entry is surfaced by a consistency check rather than adopted.
7. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- code: memory-api/crates/ticket-api/src/model/filesystem.rs
- code: memory-api/crates/ticket-api/src/model/schema_registry.rs
- code: memory-api/crates/memory-api/src/model/entity.rs