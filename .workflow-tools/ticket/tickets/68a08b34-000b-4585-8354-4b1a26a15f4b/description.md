# Problem

`ticket board show` and `ticket next` are not scope-aware enough for multi-root repositories.

In this repository, relevant tickets lived under multiple ticket roots:

- `.ticket/`
- `memory-viewers/.ticket/`
- `viewer-api/.ticket/`

Running `ticket board show --json` from the repo root reported a clear board and recommended unrelated work (`crane-cli`) while I was asking for the next tickets for the doc-viewer codebase. The output did not make the active ticket root or scan-root scope obvious, so the command looked authoritative while answering the wrong question.

# Scope

1. Surface the active ticket root / workspace scope in `board show`, `next`, and other discovery commands.
2. Add an explicit scope selector for discovery flows, such as `--root`, `--workspace`, or similar, so users can ask for the next tickets for a specific codebase.
3. In human-readable output, clearly state whether results are repo-wide, root-local, or aggregated across multiple registered roots.
4. In JSON output, expose machine-readable scope metadata so clients do not need to infer it.
5. Carry the same scope metadata through CLI, MCP, and any frontend-facing next-workflow transport so ticket-viewer / ticket-vscode do not invent parallel notions of workspace scope.

# Regression Validation Requirements

- **Specification / docs:** update the canonical next-workflow contract and operator-facing docs to define root discovery, explicit scope selection, and output fields for active scope metadata.
- **CLI:** add integration coverage for repo-root and nested-root invocation from `.ticket/`, `memory-viewers/.ticket/`, and `viewer-api/.ticket/`.
- **MCP:** add parity coverage so `next_tickets` and related discovery responses expose the same scope semantics.
- **Frontends:** any ticket-viewer / ticket-vscode next-work surface must display the active scope/root label from backend data rather than reconstructing it client-side.
- **Manual validation:** include the original doc-viewer scenario, executed from repo root and nested roots, and confirm the selected scope and recommended tickets agree.

# Acceptance Criteria

- `ticket board show --json` includes the active ticket root and the scan roots being considered.
- `ticket next` supports an explicit scope selector that can target `memory-viewers` or another nested ticket root.
- Human-readable `board show` and `next` output clearly state the scope being queried.
- A scoped doc-viewer query no longer recommends unrelated top-level tickets when the relevant work lives under a nested ticket root.
- Canonical spec / docs are updated to describe scope semantics and the new output fields.
- CLI and MCP regression tests cover repo-root and nested-root scenarios.
- Manual validation checklist includes repo-root, nested-root, and doc-viewer examples.

# Likely Surfaces

- `crates/ticket-api/`
- `tools/ticket-cli/`
- `tools/ticket-mcp/`
- `memory-api/.spec/`
- `memory-api/README.md`