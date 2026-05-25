# Problem

The proposed rule-target-backed spec workflow is still theoretical until at least one real spec stops duplicating canonical prose and proves the migration path end to end.

## Desired outcome

One real spec uses rule targets for `body.md` and at least one section, and the repository documents how to repeat that migration.

## Proposed direction

- Use `rule import-file` or an equivalent workflow to extract reusable prose from an existing spec artifact.
- Curate the resulting canonical rules, define the target outputs, and wire the spec descriptor to those targets.
- Document how authored `spec.toml` metadata stays local while generated prose moves into canonical rule content.

## Acceptance criteria

- At least one non-trivial spec body or section is generated from rule targets end to end.
- The migrated spec documents which artifacts are generated and how to regenerate them.
- Repo docs explain how to migrate authored spec prose into canonical rules without moving `spec.toml` ownership.
- Validation covers the migration flow, not just the individual helper APIs.
