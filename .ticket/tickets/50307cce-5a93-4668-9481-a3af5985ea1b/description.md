## Problem

The VS Code extension starts the ticket server without `--index-root` when no `.ticket` workspace is detected. That extension-equivalent path must be covered so future changes cannot reintroduce implicit workspace creation.

## Scope

Add an integration or E2E test that exercises the launch shape produced by ticket-vscode when `detectTicketWorkspaces()` returns no stores.

## Acceptance Criteria

- The fixture repository has no `.ticket` directory before launch.
- The test derives or mirrors `resolveServerLaunch()` behavior for the no-detected-workspace case: cwd is the repository root and no `--index-root` is passed.
- The spawned server becomes reachable on a dynamic port or reports a controlled startup failure without creating `.ticket`.
- Provider-style HTTP probes (`/api/workspaces`, `/api/tickets?workspace=default`) do not create `.ticket`.
- The test asserts no sibling memory store roots are created unexpectedly.
- The test is documented as the regression reproducer for ticket-vscode auto-start behavior.
