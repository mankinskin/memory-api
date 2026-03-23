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

## Assertions

- Prefer assertions that check behavior, not incidental implementation details.
- Keep regression tests focused on the bug or contract being changed.
