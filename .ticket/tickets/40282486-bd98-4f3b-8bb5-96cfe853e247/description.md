Users can ask which tickets were unblocked by finishing a dependency, but the current CLI requires a manual topgraph plus per-ticket health fan-out to answer that question.

Add a first-class `ticket unblocked-by <id>` command that resolves reverse dependencies from the supplied ticket, filters to the dependents that are now actionable, and returns a compact machine-readable + human-readable summary.

Scope:
- add the CLI command and contract text
- reuse existing traversal/ranking helpers where possible
- add focused regression coverage for actionable vs still-blocked dependents

Out of scope:
- HTTP or MCP surfaces
- retroactive event-history or state-transition delta tracking