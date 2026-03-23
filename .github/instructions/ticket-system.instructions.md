---
description: "Use when editing ticket system crates/tools. Covers state transitions, storage/search boundaries, and workflow expectations."
applyTo: "crates/ticket-api/**,tools/ticket-*/**"
---

# Ticket System Guidance

## Scope

Applies to:
- `crates/ticket-api/`
- `tools/ticket-cli/`
- `tools/ticket-http/`
- `tools/ticket-mcp/`
- `tools/ticket-viewer/`

## Design Constraints

- Respect ticket lifecycle/state machine invariants.
- Keep storage/index behavior backward compatible unless explicitly requested.
- Preserve clear separation between API, storage, transport, and UI layers.

## Workflow Expectations

- For larger ticket-system work, start with ticket planning workflow first.
- Search existing tickets/issues before introducing new behavior.
- Record acceptance criteria for behavior changes.

## Validation

- Prefer focused tests for changed modules before broader suites.
- Verify search/index behavior when touching ticket query paths.
- Confirm no regressions in CLI or MCP-facing flows for changed endpoints.
