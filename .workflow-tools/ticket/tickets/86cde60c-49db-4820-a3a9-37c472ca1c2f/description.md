# Problem

Deferred, parent, and roadmap-style tickets look too much like actionable implementation tickets.

During discovery, these tickets all showed up as `state = "new"` even though they meant very different things:

- an optional / deferred migration ticket
- a parent roadmap ticket describing multiple phases
- a concrete implementation slice that could plausibly be started now

That made `search` and `next` harder to interpret and forced manual reading of `description.md` just to determine whether a ticket was actually actionable.

# Scope

1. Introduce a clearer classification for ticket intent, such as `kind = epic | plan | implementation`, `actionable = true | false`, or an equivalent schema-backed field.
2. Give deferred work a first-class representation instead of overloading `new`.
3. Update discovery commands to surface or filter by this classification.
4. Preserve backward compatibility for existing tickets and indexes.
5. Ensure frontend consumers can display the classification so deferred / meta work does not masquerade as best-next implementation work.

# Regression Validation Requirements

- **Specification / docs:** define the classification model, its storage/backward-compat rules, and how `next` treats each category.
- **CLI:** add integration tests showing that deferred and parent tickets are distinguishable from concrete implementation tickets in discovery output.
- **MCP / HTTP:** add parity tests so the classification is preserved across machine-readable surfaces.
- **Frontends:** ticket-viewer / ticket-vscode should render classification badges or equivalent affordances once the data is exposed.
- **Manual validation:** include a scenario where deferred and implementation tickets share similar titles, and verify the best-next workflow still picks the actionable slice.

# Acceptance Criteria

- A deferred ticket does not look identical to a concrete implementation ticket in discovery output.
- Parent / roadmap tickets can be identified and optionally excluded from default actionable queues.
- `search`, `list`, and `next` expose the classification in JSON output.
- Existing tickets can be migrated or interpreted without breaking current storage behavior.
- Canonical spec / docs define how the classification influences best-next discovery.
- CLI and MCP regression tests cover deferred, parent, and implementation examples.
- Manual validation checklist covers the confusing real-world cases that prompted this ticket.

# Likely Surfaces

- `crates/ticket-api/`
- `tools/ticket-cli/`
- `tools/ticket-mcp/`
- `memory-api/.spec/`