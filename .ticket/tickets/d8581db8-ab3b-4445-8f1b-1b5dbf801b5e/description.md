# Problem

`rule-api` currently stores canonical rule body text in two places:

- the `body` field inside `rule.toml`
- the markdown asset file written beside the manifest

That duplication makes the manifest heavier than it needs to be, creates stale-body risk when one source changes without the other, and leaves rule folders inconsistent with the repository's existing `body.md` convention used by `spec-api`.

## Scope

- Define the canonical `rule-api` on-disk rule folder contract as `rule.toml` metadata plus `body.md` content plus history/feedback/assets.
- Update the owning `memory-api` rule-api specs so they describe `body.md` as canonical and stop documenting `description.md` for rules.
- Document the compatibility window for legacy rule folders that still have `description.md` and/or a manifest-level `body` field.
- Document the rollout and mismatch-handling rules for migrating the existing rule stores.

## User Stories

- As a rule author, I can see one obvious place to edit rule prose without touching metadata.
- As an implementer, I can follow a spec that separates persistent rule metadata from persistent rule body content.
- As a reviewer, I can validate the migration contract before the storage code and bulk rewrite land.

## Acceptance Criteria

- The owning rule-api spec set defines `body.md` as the canonical rule body file.
- The rule storage contract no longer requires `body` inside `rule.toml` metadata.
- The compatibility behavior for legacy `description.md` folders and manifest `body` fields is explicitly documented.
- The migration rollout covers the root, `memory-viewers`, `memory-api`, and `viewer-api` rule workspaces and defines what happens when manifest body text and file body text disagree.
- The spec update lists the focused validation commands expected for implementation review.