Expose TicketStore::write_part, write_amendment_part, and undo_part (landed in 5a3d152c/3d952036/f9e70385) over ticket-cli, ticket-mcp, and ticket-http transports. Currently zero transport surface exists — grepping for write_part/part_id across ticket-cli, ticket-http, ticket-mcp returns nothing, so the "freeze + add review note" acceptance criterion cannot be exercised outside Rust unit tests.

Scope: list parts, get part by id, write part (new or update via --part-id), write amendment (--supersedes), undo part. Must route through existing store methods (enforce_part_write_gate applies). Do not change ticket-api semantics or touch ticket-viewer.


## Implementation summary

Added transport surface across ticket-cli, ticket-mcp, ticket-http for TicketStore::write_part / write_amendment_part / undo_part / TicketFs::load_parts, without changing ticket-api semantics.

Files changed:
- memory-api/tools/cli/ticket-cli/src/cli/args/operations.rs: ListPartsArgs, GetPartArgs, WritePartArgs, WriteAmendmentArgs, UndoPartArgs.
- memory-api/tools/cli/ticket-cli/src/cli/commands/parts.rs (new): cmd_list_parts, cmd_get_part, cmd_write_part, cmd_write_amendment, cmd_undo_part.
- memory-api/tools/cli/ticket-cli/src/cli/commands/crud.rs: made resolve_author pub(crate) for reuse.
- memory-api/tools/cli/ticket-cli/src/cli/commands/mod.rs: registered parts module.
- memory-api/tools/cli/ticket-cli/src/cli.rs: added list-parts/get-part/write-part/write-amendment/undo-part subcommands.
- memory-api/tools/cli/ticket-cli/src/cli/dispatch.rs: wired new commands into store dispatch + dry-run payloads + descendant scan roots (read ops only).
- memory-api/tools/mcp/ticket-mcp/src/server/types.rs: ListPartsInput, GetPartInput, WritePartInput, WriteAmendmentInput, UndoPartInput.
- memory-api/tools/mcp/ticket-mcp/src/server/parts.rs (new): 5 tool implementations.
- memory-api/tools/mcp/ticket-mcp/src/server.rs: registered parts module + 5 #[tool] entries.
- memory-api/tools/mcp/ticket-mcp/src/server/workflow.rs: added new tools to help_tool catalog.
- memory-api/tools/mcp/ticket-mcp/src/lib.rs: raised recursion_limit to 256 (help_tool json! macro depth).
- memory-api/tools/http/ticket-http/src/serve/handlers/tickets/types.rs: PartItem, ListPartsResponse, PartResponse, WritePartBody, WriteAmendmentBody, ListPartsParam.
- memory-api/tools/http/ticket-http/src/serve/handlers/tickets/parts.rs (new): list_parts/get_part/write_part/write_amendment/undo_part handlers.
- memory-api/tools/http/ticket-http/src/serve/handlers/tickets.rs: registered parts submodule.
- memory-api/tools/http/ticket-http/src/serve/handlers/tickets/mutations.rs: made author_from_headers pub(super) for reuse.
- memory-api/tools/http/ticket-http/src/serve/routes.rs: 5 new /api/tickets/{id}/parts... routes.
- memory-api/tools/http/ticket-http/src/serve/error.rs: classified FrozenPartWrite as a 409 client error surfacing the full message verbatim (previously unreachable!() for this variant).

CLI surface:
- ticket list-parts <id> [--with-content] [--json|--toon]
- ticket get-part <id> --part-id <uuid>
- ticket write-part <id> [--part-id <uuid>] --kind <kind> (--content <text> | --content-file <path>) [--author <name>]
- ticket write-amendment <id> --supersedes <part-id> [--part-id <uuid>] (--content <text> | --content-file <path>) [--author <name>]
- ticket undo-part <id> --part-id <uuid> [--author <name>]

MCP tools: list_parts, get_part, write_part, write_amendment, undo_part (schemas in types.rs).

HTTP routes: GET /api/tickets/{id}/parts, GET /api/tickets/{id}/parts/{part_id}, POST /api/tickets/{id}/parts, POST /api/tickets/{id}/parts/amendment, POST /api/tickets/{id}/parts/{part_id}/undo.

Validation: cargo build --workspace clean; cargo test -p ticket-api -p ticket-cli: 289 passed; cargo test -p ticket-mcp: 8/9 passed (1 pre-existing unrelated failure: update_ticket_blocked_transition_reports_recovery_fields asserts stale 'new' state name, not touched by this change); cargo test -p ticket-http: pre-existing "workspace should open" registry failures only (7), unrelated. Manual 8-step CLI proof on scratch ticket ecffa4c7 in /tmp/parts-e2e: create (open) -> write objective+acceptance_criteria -> transition planned (5 planning parts frozen=true) -> frozen objective write REJECTED with full FrozenPartWrite text, hash byte-identical before/after -> review part write SUCCEEDED while planned -> amendment superseding frozen objective succeeded, list showed both -> transition back to open, all parts frozen=false -> delete + scan diagnostics empty. Workspace cleaned up after proof.