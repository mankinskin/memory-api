<!-- aligned-structure:v1 -->

# Summary

The ticket CLI needs first-class reverse-dependency workflow support: `ticket unblocked-by <id>` should show which dependents a prerequisite unlocks or still affects, and `ticket next <id>` should show the next actionable blocker tickets to finish in order to advance those affected dependents.

## Behavior Story

The ticket CLI needs first-class reverse-dependency workflow support: `ticket unblocked-by <id>` should show which dependents a prerequisite unlocks or still affects, and `ticket next <id>` should show the next actionable blocker tickets to finish in order to advance those affected dependents.

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

The ticket CLI needs first-class reverse-dependency workflow support: `ticket unblocked-by <id>` should show which dependents a prerequisite unlocks or still affects, and `ticket next <id>` should show the next actionable blocker tickets to finish in order to advance those affected dependents.

## Required behavior

- `ticket unblocked-by <id>` resolves the supplied ticket id or prefix using the same rules as other ticket commands.
- The command treats the supplied ticket as satisfied for the purpose of unlock discovery, regardless of its current state, so callers can ask what that ticket would unblock before it reaches a terminal state.
- The command walks reverse `depends_on` edges from the supplied ticket and considers both direct and transitive dependents.
- Returned items include only non-terminal, non-paused dependents whose remaining `depends_on` blockers are already satisfied once the supplied ticket is treated as satisfied.
- The JSON payload includes the queried root ticket, the total reachable reverse dependents, the count of still-blocked dependents in that reachable slice, the list of actionable dependents, and a separate list of impacted dependents that remain blocked after this prerequisite is satisfied.
- Returned items preserve the same ranking metadata as `ticket next` and include `remaining_blocker_count`, which is zero for actionable dependents.
- Still-blocked impacted dependents include their `remaining_blocker_count` and `remaining_blockers` so callers can see why the queried ticket was not sufficient on its own.
- `ticket next` continues to return the global actionable work list when no root ticket is supplied.
- `ticket next <id>` resolves the supplied ticket id or prefix, treats that ticket as satisfied, walks reverse `depends_on` edges to find reachable dependents, and scopes the usual `ticket next` ranking to the remaining blocker tickets of those reachable dependents.
- Root-scoped `ticket next <id>` preserves the existing ranking and board-awareness behavior of `ticket next` while also reporting the queried root ticket and reverse-dependency counts for the scoped analysis.
- Default non-JSON output renders actionable dependents with the existing compact recommendation card format used by `ticket next`.
- Default non-JSON output also renders still-blocked impacted dependents as a dedicated section instead of a raw structured object dump.

## Acceptance criteria

- A dependent ticket blocked only by the queried ticket is returned.
- A dependent ticket with at least one other unresolved blocker is excluded from the actionable `items` list and surfaced in `still_blocked_items` instead.
- A transitive dependent is returned only after all blockers along its dependency chain are satisfied.
- The command accepts a queried ticket in any current state and treats it as the satisfied prerequisite for the query.
- Impacted dependents that remain blocked are still surfaced separately with their remaining blocker counts.
- `ticket next <id>` returns actionable remaining blocker tickets for the reverse dependents reachable from `<id>` instead of the blocked dependents themselves.
- `ticket next <id>` excludes blocker tickets that are not currently actionable under the normal dependency-satisfied and board-aware `next` rules.
- Human-readable output does not fall back to the generic `[items]` object dump.
- Human-readable output does not fall back to a generic `[still_blocked_items]` object dump.

## Traceability

- Tracking ticket: `.ticket/tickets/40282486-bd98-4f3b-8bb5-96cfe853e247`
- CLI subcommand surface: `tools/cli/ticket-cli/src/cli.rs`
- Next argument surface: `tools/cli/ticket-cli/src/cli/args/operations.rs`
- CLI implementation: `tools/cli/ticket-cli/src/cli/commands/ops/next.rs`
- Human-readable renderer: `tools/cli/ticket-cli/src/cli/human_output.rs`
- Exported command schema: `crates/ticket-api/src/contracts/command_schema.rs`

## Validation

- `cargo test --manifest-path tools/cli/ticket-cli/Cargo.toml unblocked_by -- --nocapture`
- `cargo test --manifest-path tools/cli/ticket-cli/Cargo.toml next -- --nocapture`
- `cargo test --manifest-path tools/cli/ticket-cli/Cargo.toml command_schema_export_is_stable -- --nocapture`
- `cargo run --quiet --manifest-path tools/cli/ticket-cli/Cargo.toml -- --index-root ../../.ticket unblocked-by 3554ee9e`
- `cargo run --quiet --manifest-path tools/cli/ticket-cli/Cargo.toml -- --index-root ../../.ticket next 3554ee9e`
- `cargo run --quiet --manifest-path tools/cli/spec-cli/Cargo.toml -- refs 0386c4d0 validate --workspace-root .`
