## Objective

Let an agent read only the parts of a ticket its role needs, via four named view profiles or an explicit part list, across the API, CLI, and MCP surfaces.

## Requirements

- Reads accept either a named `view` profile or an explicit `parts` list. Passing both is an error.
- Profiles are role-shaped bundles:
  - `summary` = metadata + `objective`
  - `plan` = metadata + `objective` + `requirements` + `design` + `examples` + `acceptance_criteria` + refs
  - `review` = metadata + `acceptance_criteria` + `review` + `validation`
  - `full` = everything, including free-form parts
- `parts=a,b,c` returns exactly the requested kinds and nothing else.
- Requesting a part kind the ticket does not have returns an empty result for that kind, not an error.
- Free-form parts appear only under `full` and under an explicit `parts` request naming them.
- Aggregated output composes parts in manifest order under stable headings so downstream parsing is deterministic.
- Exposed identically on `get_ticket` (MCP), `ticket get` / `ticket describe` (CLI), and the ticket-api read surface. `--toon` output is supported.
- Default when neither is passed: `summary`, so an unqualified read is cheap rather than pulling the whole ticket.
- A ticket with no `[[parts]]` table still reads correctly: `description.md` is treated as the sole implicit `objective` part for `summary`, `plan`, `review`, and `full` projections that would otherwise include the objective.

## Design

The projection layer sits above `TicketFs::read` and `TicketFs::read_description`: read callers do not need to know whether the ticket is legacy or structured, only which profile or part list they want. `TicketStore::get`, `memory-api/tools/mcp/ticket-mcp/src/server/query.rs`, `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/read.rs`, `memory-api/tools/cli/ticket-cli/src/cli/commands/crud.rs`, and the ticket-viewer read path in `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/page.rs` all consume the same projection helper so the selection semantics do not drift per transport.

Profiles carry the value here: they encode which bundle matches which agent job, so the common path is one short flag rather than a memorised vocabulary. The explicit list is the escape hatch for cases no profile covers. A general query expression language is explicitly out of scope — profiles plus a part list are the whole projection surface.

Amendment rendering is an open question inherited from the interview: amendments may render inline after the part they supersede, or in a separate trailer. Decide it here and record the decision as an `amendment`-free design note before implementation.

## Implementation Steps

1. Add a projection helper in `memory-api/crates/ticket-api/src/storage/store.rs` that returns the selected manifest parts for `summary`, `plan`, `review`, `full`, or an explicit part list.
2. Update `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/read.rs` and its request/response types so `view` and `parts` flow through the HTTP read surface.
3. Update `memory-api/tools/mcp/ticket-mcp/src/server/query.rs`, `memory-api/tools/mcp/ticket-mcp/src/server/types.rs`, and the MCP workflow schema so `get_ticket` and `get_ticket_description` expose the same read options.
4. Update `memory-api/tools/cli/ticket-cli/src/cli/commands/crud.rs` and the CLI arg types so `ticket get` and `ticket describe` can request a profile or explicit parts.
5. Update `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/api/backend.rs`, and `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/page.rs` so the viewer uses the same projection helper and default profile.
6. Add a legacy-load regression test proving a ticket with no `[[parts]]` table treats `description.md` as the sole `objective` part.

## Examples

```bash
ticket get 5a3d152c --view summary        # orientation: metadata + objective
ticket get 5a3d152c --view plan --toon    # implementing: the whole planning unit
ticket get 5a3d152c --view review         # verifying: criteria + reviews + evidence
ticket get 5a3d152c --parts objective,acceptance_criteria
```

## Acceptance Criteria

1. Each of the four profiles returns exactly the parts tabulated above, verified against a fixture ticket carrying every core kind plus one free-form kind.
2. `parts=a,b,c` returns exactly those kinds; a requested-but-absent kind yields an empty entry, not an error.
3. Passing both `view` and `parts` is rejected.
4. A read with neither returns `summary`.
5. A free-form part is absent from `summary`, `plan`, and `review`, and present in `full`.
6. Aggregated output is byte-identical across two reads of an unchanged ticket.
7. The same four profiles are reachable from MCP, CLI, and the ticket-api read surface, and `--toon` renders each.
8. A ticket with no `[[parts]]` table still reads correctly, with `description.md` treated as the sole `objective` part for legacy compatibility.
9. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- code: memory-api/tools/mcp/ticket-mcp/src/server.rs