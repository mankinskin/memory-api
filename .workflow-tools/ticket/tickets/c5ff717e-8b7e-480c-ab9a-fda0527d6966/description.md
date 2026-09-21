# CLI / API surfaces for policy management

## Goal

Expose commands to inspect and edit the workspace policy and to rescan with policy applied.

## Commands

- `ticket workspace policy show` — print the resolved policy (with source: file vs compatibility defaults).
- `ticket workspace policy set --include-descendants <bool> --include-ancestors <bool>` (and optionally `--deny-external-paths <bool>`).
- `ticket workspace ignore add <path-or-glob>` / `ticket workspace ignore remove <path-or-glob>`.
- `ticket workspace include add <path-or-glob>` / `ticket workspace include remove <path-or-glob>`.
- `ticket workspace rescan --apply-policy` — re-run discovery + scan, re-registering scan roots with fresh `policy_decision` metadata.

## Scope

- Add a `Workspace` subcommand group to the CLI enum in [cli.rs](memory-api/tools/cli/ticket-cli/src/cli.rs) (nested `policy` / `ignore` / `include` / `rescan` subcommands).
- Dispatch handlers in [dispatch.rs](memory-api/tools/cli/ticket-cli/src/cli/dispatch.rs) that load/mutate `.ticket/workspace-policy.toml` (create the file on first `set`/`add`, preserving unspecified fields) and re-serialize deterministically.
- `set`/`add`/`remove` must round-trip through the `WorkspacePolicy` type (ticket 1/6) rather than raw text editing.
- Support the standard `--toon` / `--json` envelope output like other commands.
- Reject unsupported operations inside `batch` if they mutate policy files (follow existing batch exclusion conventions).

## Non-goals

- Policy semantics (tickets 1–4).

## Acceptance criteria

- [ ] `policy show` renders resolved policy with source and `--toon`/`--json` support.
- [ ] `policy set` and `ignore/include add|remove` persist to `.ticket/workspace-policy.toml`, preserving other fields.
- [ ] `rescan --apply-policy` re-registers scan roots with correct `policy_decision` metadata and reports skipped roots.
- [ ] CLI integration test covers show → set → ignore add → rescan.
- [ ] `cargo test -p ticket-cli` passes.

## Files

- [memory-api/tools/cli/ticket-cli/src/cli.rs](memory-api/tools/cli/ticket-cli/src/cli.rs)
- [memory-api/tools/cli/ticket-cli/src/cli/dispatch.rs](memory-api/tools/cli/ticket-cli/src/cli/dispatch.rs)