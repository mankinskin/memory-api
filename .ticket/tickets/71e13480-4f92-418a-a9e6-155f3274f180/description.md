## Objective

Make the new structure the documented default across agent-facing guidance, so agents stop writing reviews into objectives and start using part-addressed writes and projected reads.

## Requirements

- This ticket owns the full guidance rewrite only: profiles, freeze contract, and role-owned part kinds. The minimal `description_mode`-required correctness fix is owned by 3d952036 and should not be re-specified here except as background.
- Agent instruction files describe the four read profiles and when each applies: `summary` to orient, `plan` to implement, `review` to verify.
- The freeze contract is documented: what freezes at `planned`, that writes to frozen parts are hard-rejected, and the two recoveries (amendment, or transition back to re-plan).
- Rule entries backing these instruction files are updated at the rule store, not only in generated output, and the generated files are regenerated from them.
- The Review Agent, Implement Agent, and Iteration Agent surfaces reference the part kinds they own: Review writes `review`, Testing writes `validation`, Implement reads `plan`.
- The ticket workflow instruction file documents the `[[refs]]` table as the way to attach external context.

## Design

This ticket exists because the structure alone does not change agent behaviour — the freeze blocks the destructive path, but nothing yet tells an agent the constructive one. Documentation lands last so it describes shipped behaviour rather than intent.

Rule-generated artifacts must be edited at the rule entry and regenerated; editing generated files directly is reverted by the next generation run. The canonical surfaces here are the ticket workflow and lifecycle instruction files plus `AGENTS.md`, with the guidance text generated from the backing rule entries rather than hand-authored once the rules change.

## Implementation Steps

1. Update the backing rule entries that generate `memory-api/.agents/instructions/ticket/workflow.instructions.md` and `memory-api/.agents/instructions/ticket/lifecycle.instructions.md` so the workflow, profiles, `[[refs]]`, and freeze contract are described in one canonical place.
2. Regenerate the corresponding instruction files from those rule entries and verify the generated files contain the new wording without manual edits.
3. Update `AGENTS.md` and any linked agent guidance snippets so review, testing, and implement roles are mapped to `review`, `validation`, and `plan` respectively.
4. Rewrite the agent-facing examples so they show part-addressed writes, `planned` freeze behavior, and the amendment recovery path instead of whole-description replacement.
5. Add a regeneration check or doc-validation step proving the generated files are stable after the rule update.
6. Record validation evidence for the documentation refresh in test-api so the ticket has explicit proof for each acceptance criterion.

## Examples

Instruction text replaced:

> before: update the ticket description with the review outcome
> after: record the review outcome as a `review` part (`ticket update <id> --part review --mode append`); the `objective` is frozen at `planned` and must not be edited

## Acceptance Criteria

1. No agent instruction file instructs writing status, review, or validation content into a ticket description.
2. The four view profiles and their intended role are documented in the ticket workflow instruction surface.
3. The freeze contract and both recovery paths are documented.
4. Every changed instruction file's backing rule entry is updated and the file is regenerated; a regeneration run produces no diff.
5. Review, Testing, and Implement agent surfaces each name the part kind they read or write.
6. A fresh agent session following only the documentation can record a review on a `planned` ticket without triggering a freeze rejection.
7. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- file: .agents/instructions/ticket/workflow.instructions.md
- file: .agents/instructions/ticket/lifecycle.instructions.md
- file: AGENTS.md