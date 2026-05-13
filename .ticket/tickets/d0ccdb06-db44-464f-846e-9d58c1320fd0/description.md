# Problem

Nested rule work and repo-local README generation need a committed spec set in `memory-viewers/memory-api/.spec` with maintained code references and validation hooks. Initial planning specs now exist, but the implementation work still needs an explicit ticket to keep those specs current as behavior lands.

## Scope

- Maintain the `memory-api` rule-api spec set:
  - `rule-api/workspaces`
  - `rule-api/workspaces/nested-resolution`
  - `rule-api/workspaces/memory-api-readme-generation`
- Keep `code_refs` aligned with the active rule-api, rule-cli, and validation test seams.
- Expand or update spec sections as nested workspace support and README generation behavior become concrete.
- Validate spec references as part of review for the implementation tickets that land this work.

## User Stories

- As an implementer, I can see the rule-api contract, repo ownership model, and validation points in one local spec set.
- As a reviewer, I can trace each planned behavior to code seams and tests without reconstructing context from tickets.
- As a future maintainer, I can update the spec set inside `memory-api` when rule generation behavior changes.

## Usage Guide

1. Update spec content under `memory-api/.spec/specs/**` when the rule-api design or behavior changes.
2. Keep `code_refs` pointed at the current implementation seams and the tests that validate them.
3. Run `cargo run -p spec-cli -- refs <spec> validate --workspace-root . --index-root target/tmp/rule-api-spec-index` from `memory-api/`.
4. Move implementation tickets to review only after the specs reflect the delivered behavior.

## Acceptance Criteria

- The three rule-api specs exist in `memory-api/.spec/specs`.
- Each spec includes validated code references to current implementation seams and tests.
- The spec set covers nested workspaces, repo-local authoring, and README generation.
- Review guidance identifies which tests or manual checks validate each feature slice.
