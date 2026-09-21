# Problem

There is no end-user frontend surface for the "best next ticket to implement" workflow.

Current state:

- `ticket-viewer` frontend currently exposes list / detail / graph flows, but no next-work endpoint or UI.
- `ticket-vscode` currently lists tickets with basic filters, but no recommended-next or why-not workflow.

That means the system can only be validated at CLI and MCP layers. For a workflow that humans and agents are supposed to trust, that is not regression-resistant enough.

# Scope

1. Add frontend-consumable backend/client plumbing for the next-workflow contract in ticket-viewer and ticket-vscode.
2. Expose a next-work view / panel / command that shows ranked candidates, current scope, board warnings, and exclusion reasons.
3. Show deferred / meta classification so parent or deferred tickets do not masquerade as actionable work.
4. Keep UI semantics aligned with CLI / MCP output; no frontend-only ranking or filter rules.
5. Add end-to-end coverage for the user-facing next-work workflow.

# Acceptance Criteria

- ticket-viewer can request and display recommended next tickets for a selected workspace/root.
- ticket-viewer can display current scope, board warnings, and why a searched ticket is absent from `next`.
- ticket-vscode offers a command or view for the same workflow and reuses the same backend semantics.
- UI surfaces display classification / deferment cues so actionable vs non-actionable work is obvious.
- Ticket-viewer release Playwright coverage and ticket-vscode test coverage cover scope switching, filter matching, omitted-ticket explanation, and deferred / meta labeling.
- Manual validation instructions cover both frontends.

# Likely Surfaces

- `memory-viewers/ticket-viewer/src/`
- `memory-viewers/ticket-viewer/frontend/dioxus/`
- `memory-api/tools/ticket-vscode/`
- `memory-api/tools/mcp/ticket-mcp/`
- `memory-api/.spec/`