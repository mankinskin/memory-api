# Goal
Research and codify the minimal, repeatable bootstrap policy for creating a new memory-backed store that can immediately perform CRUD, validate entity schemas, participate in workspace hierarchy resolution, and create/reference entities across stores.

## Why this work exists
Creating a new store today still requires rediscovering patterns scattered across `ticket-api`, `spec-api`, `rule-api`, `session-api`, and workspace-resolution work. We need one policy and eventually one template/test fixture proving the smallest viable setup.

## Research questions
- What are the actual minimum moving parts for a new store: registry, schema defaults, CRUD storage, index/search bootstrap, CLI/HTTP/MCP surfacing, and health checks?
- Which capabilities belong in generic `memory-api` primitives versus domain-owned crates?
- What is the smallest executable template crate or fixture workspace that proves create/get/update/delete out of the box?
- How should cross-store references and reverse references be modeled so new stores can be linked from tickets/specs/logs/tests without bespoke glue?
- Which existing workspace-resolution and scan-root rules are mandatory for nested workspaces?

## Starting anchors
- `memory-api` tracker precedent: `39239e48-828a-41d8-a697-9cf02e980da9` (`Transport-layer workspace-resolution parity (tracker)`)
- draft spec seed: `9ee9387f-5384-42a9-95c4-ecbad1713030` (`scaffold: rule-generated domain-store bootstrap instructions and slash skill`)
- schema surfaces found during this session: `memory-api/crates/rule-api/src/default_schema.rs`, `memory-api/tools/http/ticket-http/src/serve/handlers/schema.rs`, `memory-api/tools/cli/ticket-cli/tests/contracts_schema_validation.rs`

## Deliverables
- comparison matrix across `ticket-api`, `spec-api`, `rule-api`, and `session-api`
- proposed minimal-store template boundary, including what would be generated versus handwritten
- list of required specs and child implementation tickets
- validation recipe proving CRUD, schema validation, cross-store reference handling, and workspace hierarchy behavior

## Validation expectations
The next session should be able to produce at least one durable artifact: either a focused spec update or a decomposition into concrete implementation tickets anchored to real crates/tests.

## Research snapshot (2026-07-09)

- Durable artifact landed: spec `9ee9387f-5384-42a9-95c4-ecbad1713030` now contains the foundational bootstrap comparison matrix across `ticket-api`, `spec-api`, `rule-api`, `test-api`, `log-api`, and `session-api` roles.
- Core-profile decision: the first canonical template targets one foundational memory-store contract for a fully operational durable store, then leaves explicit extension hooks for domain-specific workflows, attached artifacts, and richer query semantics.
- `session-api` is no longer treated as the archetypal exclusion case. Foundational durable-store mechanics still apply where session records are stored, while session planning, capture layout, and transcript artifacts remain extension behavior.
- Reusable bootstrap anchors are now explicit: domain schema registration or equivalent shape rules, transport exposure, schema-validation tests, workspace-resolution behavior, and policy-aware cross-store links.
- Validation completed: `spec.exe refs 9ee9387f-5384-42a9-95c4-ecbad1713030 validate --workspace-root . --toon` returned `valid: true`.

## Current follow-up gap

The policy draft is settled enough to move into execution. The repo now has concrete implementation ticket `e268a1e8-3f3a-433f-b4a0-d58c590b8d29` for the first core-profile fixture or template smoke path, and follow-up work should stay on proving CRUD, schema validation, cross-store references, nested-workspace discovery, and claimed baseline transport wiring rather than reopening policy discovery.
