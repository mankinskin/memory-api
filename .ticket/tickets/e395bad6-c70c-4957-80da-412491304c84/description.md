# Problem

Even after the desired `body.md` contract is clear, `rule-api` still depends on shared storage helpers that hardcode `description.md`, and the rule schema still requires a manifest-level `body` field. Without a compatibility slice, any direct rename risks breaking scans, search hydration, CLI/MCP responses, or generation for the 543 existing rule folders.

## Scope

- Extend the shared entity folder configuration so domains can choose their body filename instead of hardcoding `description.md`.
- Make `rule-api` use `body.md` as its canonical body asset while leaving `ticket-api` on `description.md` and preserving `spec-api` behavior.
- Update `rule-api` hydration, search indexing, create/update paths, and scan/open paths to read `body.md` first, then fall back to legacy `description.md`, then fall back to manifest `body` only when necessary for compatibility.
- Remove the schema requirement that forces a persisted `body` field in `rule.toml`, while keeping the CLI/MCP/user-facing payloads able to surface a rule body during the compatibility window.
- Add focused tests for create, update, reopen/scan, and generated rendering under both migrated and legacy folder layouts.

## User Stories

- As a maintainer, I can switch rule storage to `body.md` without changing ticket or spec storage semantics.
- As a CLI or MCP caller, I still receive body text for a rule during the migration instead of silent regressions.
- As a reviewer, I can see compatibility covered by focused tests before the bulk filesystem migration runs.

## Acceptance Criteria

- Newly created or updated rule entries write body content to `body.md`, not `description.md`.
- `rule-api` can still read legacy rule folders that contain `description.md` and/or manifest-level `body` content.
- Search, list/get, update, and markdown generation continue to work for both legacy and migrated rule folders.
- Ticket and spec storage behavior is unchanged.
- Focused `rule-api`, `rule-cli`, and `rule-mcp` validation covers the compatibility paths introduced by the slice.