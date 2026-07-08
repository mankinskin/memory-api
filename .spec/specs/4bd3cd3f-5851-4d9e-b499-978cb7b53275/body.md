<!-- aligned-structure:v1 -->

# Summary

The current best-next contract is deterministic but shallow: default next discovery ranks only dependency-satisfied candidates by candidate workflow state, priority, immediate dependees, and recency. That keeps CLI, board, and MCP behavior aligned, but it does not model convergence pressure when an earlier-state prerequisite is holding up a more advanced dependent.

## Behavior Story

The current best-next contract is deterministic but shallow: default next discovery ranks only dependency-satisfied candidates by candidate workflow state, priority, immediate dependees, and recency. That keeps CLI, board, and MCP behavior aligned, but it does not model convergence pressure when an earlier-state prerequisite is holding up a more advanced dependent.

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

The current best-next contract is deterministic but shallow: default next discovery ranks only dependency-satisfied candidates by candidate workflow state, priority, immediate dependees, and recency. That keeps CLI, board, and MCP behavior aligned, but it does not model convergence pressure when an earlier-state prerequisite is holding up a more advanced dependent.

The repository already has manual escape hatches for this problem. `ticket next <root>` and `unblocked-by <root>` can walk reverse dependencies and identify remaining blocker work for a specific prerequisite chain. The gap is that the default ranking, health findings, and repo audit do not reuse that dependency model, so the system does not automatically steer back toward prerequisite-first execution when work drifts out of order.

## Goals

- Extend best-next ranking beyond immediate reverse degree so default next discovery promotes eligible prerequisites that unblock more advanced dependents.
- Define one reusable dependency-convergence model owned by `ticket-api` and shared by ranking, health, and audit consumers.
- Keep the contract deterministic and explainable across CLI and MCP surfaces.
- Keep dedicated HTTP `GET /api/next` work separate; the HTTP route consumes this ranking contract after the model is finalized.

## Required behavior

### Relationship to the current contract

- This spec is the implemented convergence extension to `ticket-api/workflow/best-next-ordering`.
- `ticket-api/workflow/best-next-ordering` remains the base fallback-order spec for cases with no convergence pressure, while this spec defines the shared dependency-convergence model and the promoted-order behavior layered on top of it.
- Any future HTTP next route must consume this contract but is tracked separately by `.ticket/tickets/181ed793-481d-4d46-b059-0eda891365d7`.

### Candidate eligibility

- Default best-next surfaces continue to return only non-terminal tickets whose own dependencies are satisfied.
- Blocked dependents are not returned as next candidates; instead, their remaining eligible blockers contribute ranking pressure to those blocker tickets.
- Implementations should reuse or align with the reverse-dependency traversal already used by `ticket next <root>` and `unblocked-by` so default ranking and scoped ranking agree on what counts as remaining blocker work.

### Derived dependency-convergence model

- `ticket-api` owns the dependency-convergence derivation and ranking inputs as library code. The graph traversal, state-gap classification, and shared metric calculation must not live independently in `ticket-cli`, `ticket-mcp`, `ticket-http`, or `audit-api`.
- `ticket-api` must expose a reusable library surface that derives convergence metrics from indexed tickets and `depends_on` edges, including both direct dependency inversions and transitive blocker-path pressure.
- The shared model must derive at least:
  - immediate dependees
  - transitive reverse-dependent count
  - affected reverse-dependent reach: how many reverse dependents would advance if the candidate completed
  - max affected dependent state: the most advanced non-terminal reverse dependent that still needs the candidate
  - dependency-state gap: the workflow distance between the candidate and that most advanced affected dependent
- The model must distinguish direct dependency inversions, where a candidate directly blocks a more advanced dependent, from transitive convergence pressure, where the candidate remains on a blocker path beneath a more advanced reverse dependent.
- `ticket next`, `ticket-mcp next_tickets`, ticket health surfaces, and repo audit findings must consume the same `ticket-api` derivation so users do not see contradictory topology classifications.
- Audit-specific severity mapping, instructions, and repo-level aggregation remain in `audit-api`, but the dependency-convergence evidence attached to those findings comes from the shared `ticket-api` model.

### Ranking order

- Default best-next ordering is a convergence-first ranking, not a pure candidate-state ranking.
- Candidates with stronger dependency-convergence pressure rank ahead of candidates that do not unblock more advanced work, even when the urgent prerequisite is in an earlier workflow state.
- `ticket-api` owns the canonical comparator inputs and deterministic ordering helper used by best-next surfaces.
- The deterministic ordering keys are:
  1. convergence pressure, ordered by:
     - higher `max_affected_dependent_state` first
     - larger `dependency_state_gap` first
     - larger `affected_reverse_dependent_reach` first
  2. explicit ticket priority
  3. candidate workflow progress
  4. transitive reverse-dependent count
  5. immediate dependees
  6. `created_at` with newer tickets first
  7. title, then one final deterministic fallback if needed
- When two candidates have no convergence pressure, the current best-next ordering may be reused for the remaining keys.

### Explainability

- CLI and MCP next payloads must surface enough derived metadata to explain why a lower-state prerequisite ranked ahead of a more advanced but less urgent candidate.
- At minimum, machine-readable next outputs must expose immediate dependees, transitive reverse impact, affected reverse-dependent reach, `max_affected_dependent_state`, and `dependency_state_gap` or an equivalent convergence field.
- The shared `ticket-api` surface should provide these derived fields so transport layers only serialize them.
- User-facing contract text must explain that default next can promote earlier-state prerequisites to restore dependency order when more advanced dependents are waiting on them.

### Health and audit consumers

- Health surfaces must produce a dedicated dependency-convergence finding when a non-terminal ticket depends on a prerequisite in a strictly earlier workflow state.
- Health and audit must call the same `ticket-api` dependency-convergence derivation rather than open-coding separate unresolved-dependency heuristics.
- Repo audit must report both topology counts and convergence-risk findings based on the same derived model rather than only orphan-ticket detection.
- Findings must include the dependent ticket, blocking prerequisite, both states, and the relevant reverse-dependent reach or state-gap metric needed to triage the issue.

## Acceptance criteria

- Given an eligible `new` or `ready` prerequisite that blocks an `in-implementation` or `in-review` dependent, default next ranks that prerequisite ahead of otherwise similar candidates that do not unblock more advanced work.
- CLI and MCP next surfaces produce the same ordering and explainability fields for equivalent candidate sets under the redesigned contract.
- Focused regression coverage proves transitive reverse-dependent pressure affects ordering beyond immediate dependees.
- Health and audit planning reuse the same `ticket-api` derived dependency model instead of inventing separate state-gap terminology or graph traversal helpers.
- The dedicated HTTP next route remains a consumer of this contract rather than redefining the ranking heuristic.

## Related specs

- `ticket-api/workflow/best-next-ordering`

## Traceability

- Tracking ticket: `.ticket/tickets/d1f9f390-dda0-4762-a14c-9ce339abc393`
- Related audit ticket: `../.ticket/tickets/95d4f986-b81c-4951-bae5-4227f2d72a6d`
- Related HTTP ticket: `.ticket/tickets/181ed793-481d-4d46-b059-0eda891365d7`
- Existing scoped reverse-dependency behavior: `.spec/specs/0386c4d0-15c4-4561-a33f-63b881c852c5`
- Workflow rustdoc: `crates/ticket-api/src/workflow/mod.rs`
- CLI operator docs: `tools/cli/ticket-cli/README.md`
- MCP operator docs: `tools/mcp/ticket-mcp/README.md`
- Audit CLI operator docs: `tools/cli/audit-cli/README.md`
- Audit MCP operator docs: `tools/mcp/audit-mcp/README.md`

## Validation

- `cargo test -p ticket-api workflow:: -- --nocapture`
- `cargo test -p ticket-cli next_and_board_promote_convergence_before_unrelated_ready_work --test integration_board_cli -- --nocapture`
- `cargo test -p ticket-cli next_with_root_returns_actionable_remaining_blockers --test integration_board_cli -- --nocapture`
- `cargo test -p ticket-cli unblocked_by_returns_only_actionable_reverse_dependents --test integration_board_cli -- --nocapture`
- `cargo test -p ticket-mcp next_tickets_ -- --nocapture`
- `cargo test -p ticket-cli health -- --nocapture`
- `cargo test -p ticket-mcp health -- --nocapture`
- `cargo test -p ticket-http health -- --nocapture`
- `cargo test -p audit-api ticket_graph -- --nocapture`
- `cargo run -p rule-cli --bin rule -- sync-targets --config rule-targets.yaml --check`
