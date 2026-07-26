<!-- aligned-structure:v1 -->

# Summary

`ticket board` should present the same flag naming style as the rest of `ticket-cli`.

## Behavior Story

`ticket board` should present the same flag naming style as the rest of `ticket-cli`.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# Summary

`ticket board` should present the same flag naming style as the rest of `ticket-cli`.

The canonical public surface favors the short forms already used by adjacent
commands:

- `--agent` for agent identity values
- repeated singular `--file` flags for owned file lists
- `--from` / `--to` for rename-style path pairs

Board-specific documentation has drifted toward longer spellings such as
`--agent-id`, `--files`, `--old-path`, and `--new-path`. The CLI should keep
accepting those spellings for compatibility, but help output and regression
tests should treat the common `ticket-cli` forms as canonical.

## Required behavior

- `ticket board show`, `history`, `check-in`, `check-out`, `update-files`, and
	`rename-file` show `--agent` in help output when an agent filter or identity
	is required.
- `ticket board check-in` shows `--file` as the canonical repeated file flag.
- `ticket board rename-file` shows `--from` and `--to` as the canonical rename
	flags.
- The parser continues to accept the longer board-specific compatibility
	spellings used by existing docs and prompts.
- No board command behavior changes beyond canonical help text and compatible
	argument parsing.

## Acceptance criteria

- Help output for the board subcommands uses the common `ticket-cli` flag names
	for agent, file-list, and rename arguments.
- Compatibility aliases for the previously documented long forms still parse.
- Focused CLI coverage locks both the canonical help text and at least one
	compatibility-alias path.

## Traceability

- Tracking ticket: `.ticket/tickets/8de93812-3a8c-4937-9f09-05a9a9b86309/ticket.toml`
- Canonical parser surface: `tools/cli/ticket-cli/src/cli/args/board.rs`
- Focused regression coverage: `tools/cli/ticket-cli/tests/integration_board_cli.rs`
- Updated workflow guidance: `.agents/instructions/ticket/`

## Validation

- `cargo test -p ticket-cli board_help_uses_canonical_common_flag_names --test integration_board_cli -- --nocapture`
- `cargo test -p ticket-cli board_long_form_aliases_still_parse --test integration_board_cli -- --nocapture`
