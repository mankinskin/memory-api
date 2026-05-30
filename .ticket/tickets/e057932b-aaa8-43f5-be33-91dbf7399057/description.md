# Problem

Once `rule-api` can understand the new storage contract, the repository still has 543 existing rule folders spread across four workspaces that need to be brought into the new layout. Leaving them mixed indefinitely makes future scans, docs, and reviews harder to reason about and keeps fixture paths pinned to the old filename.

## Scope

- Migrate existing `.rule/rules/**` folders in the root workspace, `memory-viewers`, `memory-api`, and `viewer-api`.
- For each rule folder, compare the legacy `description.md` content with any manifest-level `body` content and stop/report mismatches instead of picking one implicitly.
- Write canonical `body.md`, remove the manifest-level `body` field from `rule.toml`, and remove legacy `description.md` when the content is unambiguous.
- Update tests, fixtures, and any hardcoded rule-body paths that still reference `description.md`.
- Re-run targeted generation/spec validation for the affected workspaces after the migration.

## User Stories

- As a maintainer, I can inspect any rule folder and find body prose in the same canonical location.
- As a reviewer, I can verify the migration did not leave silent mixed-state folders behind.
- As a future implementer, I do not need to preserve rule-specific `description.md` compatibility forever.

## Acceptance Criteria

- All existing rule folders in the four known rule workspaces are migrated to canonical `body.md`, or any blocked mismatches are surfaced explicitly.
- No migrated rule folder keeps a stale `description.md` copy.
- No migrated `rule.toml` keeps a duplicated `body` field.
- Fixture/test paths that used `.rule/rules/**/description.md` are updated.
- Focused tests and targeted post-migration validation commands pass for the affected rule workspaces.