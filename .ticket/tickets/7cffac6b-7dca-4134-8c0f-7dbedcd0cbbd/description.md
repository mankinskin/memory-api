# Problem

`memory-api` does not yet have a repo-local `.rule` workspace or a local `rule-targets.yaml`, so its `README.md` remains a manually maintained file instead of a generated target owned by the repo that the README describes.

## Scope

- Add `memory-viewers/memory-api/.rule/rules/**` and a repo-local `memory-viewers/memory-api/rule-targets.yaml`.
- Create canonical local rule entries for the `memory-api` README sections.
- Wire a local target that renders `memory-viewers/memory-api/README.md`.
- Ensure generation, sync, and stale-output cleanup work from the `memory-api/` repo root.
- Align implementation with spec `rule-api/workspaces/memory-api-readme-generation`.

## User Stories

- As a `memory-api` maintainer, I can update README sections by editing local rules next to the code.
- As a reviewer, I can regenerate the README and verify that no manual drift remains.
- As a parent repo maintainer, I can later incorporate `memory-api` rules into broader docs without reauthoring them upstream.

## Usage Guide

1. Edit README rules in `memory-api/.rule/rules/**`.
2. Run `rule explain-target --config rule-targets.yaml --target memory-api-readme`.
3. Run `rule sync-targets --config rule-targets.yaml`.
4. Validate the generated README and target-tracking tests before review.

## Acceptance Criteria

- `memory-api` has a local `.rule` workspace and README target config.
- `README.md` is rendered from local rules and regenerates cleanly.
- Generated target bookkeeping handles path changes and stale outputs.
- Tests cover target generation, sync, and stale-output pruning for the memory-api README target.
