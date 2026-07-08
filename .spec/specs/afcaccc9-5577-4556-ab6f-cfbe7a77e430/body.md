<!-- aligned-structure:v1 -->

# Summary

Define the authoritative contract for ticket cross-workspace move behavior over the shared memory-api move kernel.

## Behavior Story

Define the authoritative contract for ticket cross-workspace move behavior over the shared memory-api move kernel.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# Goal
Define the authoritative contract for ticket cross-workspace move behavior over the shared memory-api move kernel.

# Problem
Move behavior existed in implementation and tickets but lacked a single owning spec with explicit acceptance criteria and validation evidence requirements.

# Scope
- Ticket entity moves between git-backed workspace stores in one worktree.
- Preflight visibility checks for inbound and outbound references.
- Journaled execution, resume, and rollback semantics.
- Fail-closed board policy for active and stale claims.
- Surface parity expectations for CLI, MCP, and implemented HTTP transports.

# Non-goals
- Cross-worktree moves.
- Cross-store transaction semantics.
- Multi-entity batch move.

# Acceptance criteria
1. Supported topologies pass preflight and rejected topologies fail with explicit blockers.
2. Dirty tracked files block move execution before file mutation.
3. Active and stale board claims block execution; historical board rows migrate when move executes.
4. Resume and rollback recover from an injected failure after file movement begins.
5. CLI and MCP expose move preflight, apply, resume, and rollback over the same kernel contract for supported move domains.
6. HTTP exposes move over the same kernel contract for domains that ship an HTTP crate; domains without an HTTP crate are not required to create one solely for move parity.
7. Destination visibility remains valid for all references involving the moved ticket.

# Traceability
- [0a510279 Generalize cross-workspace move into a domain-neutral kernel](memory-api/.ticket/tickets/0a510279-5482-4c4f-8cb5-fad3baa57427/ticket.toml)
- [da27c074 Validate cross-workspace ticket move flows end to end](memory-api/.ticket/tickets/da27c074-8c9e-4613-b8b9-bf02c72b50f7/ticket.toml)
- [44abe1d4 Move ticket 694d74b4 into the memory-api workspace store](memory-api/.ticket/tickets/44abe1d4-5727-45f8-be3b-d1ca5bf4c1ae/ticket.toml)
- [94a51f30 Adopt generic move kernel across domains and transports](memory-api/.ticket/tickets/94a51f30-8c37-4ea6-b49a-97206d28add3/ticket.toml)
- [a7f19a7d Refresh move matrix and benchmark evidence](memory-api/.ticket/tickets/a7f19a7d-42d0-48b7-b89b-98de3c6fa3b4/ticket.toml)

# Validation evidence
- rtk cargo test -p ticket-api resume_move_with_journal_recovers_after_injected_file_move_failure -- --nocapture
- rtk cargo test -p ticket-api rollback_move_with_journal_recovers_after_injected_file_move_failure -- --nocapture
- rtk cargo test -p ticket-mcp --test integration_move_mcp -- --nocapture
- rtk cargo test -p ticket-cli cmd_move_dry_run_returns_preflight_plan -- --nocapture
- rtk cargo test -p ticket-http move_ticket_ -- --nocapture
- rtk cargo test -p spec-cli dispatch_move_dry_run_returns_supported_preflight_plan -- --nocapture
- rtk cargo test -p spec-mcp spec_move_preflight_returns_supported_plan -- --nocapture
- rtk cargo test -p spec-http move_spec_dry_run_returns_supported_plan -- --nocapture
- rtk cargo test -p rule-cli move_command_dry_run_returns_supported_preflight_plan -- --nocapture
- rtk cargo test -p rule-mcp rule_move_preflight_returns_supported_plan -- --nocapture

# Follow-up
Cross-domain adapter parity and matrix/benchmark expansion remain tracked in the linked follow-up tickets above. Future HTTP parity for `rule-api`, `audit-api`, or `session-api` requires a separate transport-creation ticket because those HTTP crates do not exist today.
