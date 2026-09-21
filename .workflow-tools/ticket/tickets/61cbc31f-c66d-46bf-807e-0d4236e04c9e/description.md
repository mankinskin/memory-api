# Problem

`ticket search` and `ticket next` do not explain their mismatch.

In practice, `ticket search "doc-viewer" --json` found relevant tickets, but only one of them appeared in `ticket next --limit 200 --json`. The missing tickets produced no explanation. I had to manually inspect ticket files to infer whether they were omitted because they were parent tickets, deferred work, or living under a different ticket root.

The tool needs a first-class way to answer: why is this ticket not in `next`?

# Scope

1. Add a `why-not` / `explain` flow for `ticket next`, for example `ticket next --explain <ticket-id-or-query>` or `ticket why-not <ticket-id>`.
2. Return structured reasons such as:
   - blocked by dependency
   - filtered out by board ownership
   - hidden because it is deferred or non-actionable
   - outside the current ticket root / workspace scope
3. Optionally annotate `search` results with next-eligibility metadata so users can move from search to action without manual cross-checking.
4. Reuse the same reason codes across CLI, MCP, and frontend consumers.

# Regression Validation Requirements

- **Specification / docs:** define the reason-code vocabulary and the user-facing contract for explaining omission from `next`.
- **CLI:** add integration tests for the core omission cases: blocked dependency, board exclusion, scope mismatch, and deferred / meta work.
- **MCP:** add parity tests so `next_tickets` or a sibling explain endpoint returns the same reason codes and related ticket IDs.
- **Frontends:** ticket-viewer / ticket-vscode next-work surfaces must be able to render the reason codes directly, without frontend-only inference.
- **Manual validation:** include a scenario where a ticket appears in `search` but not in `next`, and verify one command or UI action explains the omission.

# Acceptance Criteria

- Given a ticket returned by `search` but absent from `next`, one command explains why.
- JSON output exposes machine-readable reason codes and related ticket IDs where relevant.
- Human-readable output points to the blocking dependency, root mismatch, or classification rule responsible for the omission.
- The explanation path works for both exact ticket IDs and small text queries.
- CLI and MCP regression tests cover the documented omission reasons.
- Manual validation checklist covers a `search` vs `next` mismatch end-to-end.

# Likely Surfaces

- `tools/ticket-cli/`
- `crates/ticket-api/`
- `tools/ticket-mcp/`
- `memory-api/.spec/`