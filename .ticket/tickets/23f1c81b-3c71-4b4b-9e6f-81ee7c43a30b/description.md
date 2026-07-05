## Problem

HTTP read probes against a configured workspace path with no on-disk `.ticket` root must not auto-create stores. The regression target is workspace-resolved read/list probes.

## Implemented Reproducer

- Added: `memory-api/tools/http/ticket-http/tests/no_auto_init_missing_workspace.rs`
- The test builds an app with `WorkspaceRegistry::single(<missing .ticket path>)` and probes:
  - `GET /api/workspaces`
  - `GET /api/tickets?workspace=<canonical>&limit=10`
- Contract assertion: both probes leave `.ticket` absent; ticket listing returns `404` for the missing workspace instead of implicitly creating/opening it.

## Review Validation 2026-07-03

- Passing: `rtk cargo test --manifest-path memory-api/tools/http/ticket-http/Cargo.toml --test no_auto_init_missing_workspace -- --nocapture`
- Result: `1 passed`.
- Validation spec recorded: `vt-review-ticket-http-no-auto-init-20260703`.

## Review Decision

Acceptance criteria are met. The earlier expected-failing reproducer is now green and confirms no-auto-init behavior for the missing workspace read probes.