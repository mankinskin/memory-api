# Problem

`ticket get <id>` was not workspace-aware and failed with a raw path error when the ticket lived under a different ticket root.

Observed behavior:

- `ticket search` returned tickets that lived under nested ticket roots.
- Running `ticket get <id> --json` from the repo root failed with `storage error: io error: Das System kann den angegebenen Pfad nicht finden. (os error 3)`.

That error does not tell the user what went wrong or how to recover. At the same time, `search`, `list`, and `next` results did not expose enough root/workspace metadata to tell me where to rerun the command.

# Scope

1. Make `ticket get` resolve ticket IDs across registered or discoverable ticket roots, or fail with an explicit root-mismatch message.
2. Add root/workspace metadata to `search`, `list`, and `next` results.
3. Provide a recovery path in the error message, such as the correct `--index-root` or `--root` to use.
4. Add regression coverage for nested `.ticket/` stores.
5. Ensure frontend consumers can surface the returned root/workspace metadata instead of hiding it.

# Regression Validation Requirements

- **Specification / docs:** document workspace/root metadata fields and the recovery path for cross-root lookup failures.
- **CLI:** add integration tests covering nested-root `get`, `search`, `list`, and `next` behavior plus human-readable recovery guidance.
- **MCP / HTTP:** add parity tests for any API surface returning tickets or ticket metadata across roots.
- **Frontends:** ticket-viewer / ticket-vscode should be able to show which root/workspace a ticket belongs to when the backend returns cross-root metadata.
- **Manual validation:** use the multi-root doc-viewer scenario and verify a user can recover from repo root without raw path spelunking.

# Acceptance Criteria

- `ticket get <id>` from the repo root either succeeds for nested-root tickets or reports the correct root and retry syntax.
- The failure mode is explicit and does not expose a raw OS path error for this case.
- `search`, `list`, and `next` JSON results include root/workspace metadata for each returned ticket.
- Tests cover at least one nested-root scenario like `viewer-api/.ticket/`.
- Canonical spec / docs define the metadata contract and recovery flow.
- Manual validation checklist covers repo-root recovery for a nested ticket.

# Likely Surfaces

- `tools/ticket-cli/`
- `crates/ticket-api/`
- `tools/ticket-mcp/`
- `memory-api/.spec/`