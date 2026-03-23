---
description: "Use when creating or editing tests. Covers tracing setup, test-log debugging, and focused validation strategy."
applyTo: "**/tests/**,**/*test*"
---

# Testing Guidance

## Tracing Setup

For tracing-based tests, initialize tracing with graph context:

```rust
let _tracing = init_test_tracing!(&graph);
```

This improves readability of tokens and graph state in logs.

## Debug Workflow

When a test fails:
1. Run targeted tests first.
2. Inspect `target/test-logs/` for full trace output.
3. Use log-viewer tooling for structured log search/filtering.

## Test Execution Strategy

- Start with nearest unit/integration tests.
- Expand to crate-level runs once local failures are resolved.
- Avoid unrelated full-workspace test runs unless required.

For frontend-impacting changes:

- Run lint and typecheck in each affected frontend package.
- Run nearest unit/component tests for changed UI code.
- Run at least one browser-based end-to-end path that covers changed UX behavior.

For viewer/API integration changes:

- Add or run assertions that verify the viewer contract with context-api or ticket-api for changed endpoints.
- For filesystem-backed behaviors, include path-handling and access-boundary assertions.

For regression fixes:

- Prefer a failing reproducer assertion before or with the fix.
- Keep regression coverage focused on the reported failure mode.

## Assertions

- Prefer assertions that check behavior, not incidental implementation details.
- Keep regression tests focused on the bug or contract being changed.
