## Problem

`memory-api/tools/mcp/mcp-toolmon/src/proxy.rs` rewrites only `params.arguments.workspace` in the `Decision::Allow` block at lines 452-492. Every other path-bearing argument still resolves against the proxy's inherited main-checkout current working directory, allowing proxied tools to silently operate on the wrong checkout. The proxy also inserts `workspace` unconditionally without proving the downstream tool declares or honors that argument.

## Scope

- Define a per-tool path-argument registry or mapping that identifies which argument names for each downstream tool represent paths.
- At the `Decision::Allow` extension point in `memory-api/tools/mcp/mcp-toolmon/src/proxy.rs` lines 452-492, rewrite every registered path argument against the resolved session worktree.
- Make insertion of `params.arguments.workspace` conditional on the downstream tool schema declaring a `workspace` argument.
- Preserve the existing `workspace` path-validation behavior in `memory-api/tools/mcp/mcp-toolmon/src/proxy.rs` lines 223-301 where applicable.

## Rejected Alternative

Do not set the downstream child process current working directory at spawn time. `memory-api/tools/mcp/mcp-toolmon/src/supervisor.rs` lines 197-223 starts one long-lived downstream process before any individual `session_id` arrives. A downstream process can serve multiple sessions, so one spawn-time current working directory cannot be correct for all calls.

## Acceptance Criteria

- [ ] A per-tool registry or mapping identifies all path-bearing argument names that mcp-toolmon rewrites.
- [ ] The `Decision::Allow` path in `memory-api/tools/mcp/mcp-toolmon/src/proxy.rs` lines 452-492 rewrites each registered path argument to the session worktree resolved for the call.
- [ ] `workspace` is inserted only when the downstream tool schema declares that argument.
- [ ] Unregistered arguments remain unchanged, and existing validation in `memory-api/tools/mcp/mcp-toolmon/src/proxy.rs` lines 223-301 remains effective.
- [ ] Tests cover a tool with `workspace`, a tool with another registered path argument, and a tool that does not declare `workspace`.