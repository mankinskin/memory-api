# Problem

`ticket next`, `ticket board show`, and `ticket-mcp next_tickets` currently use the dependees-first contract documented in the current best-next spec: candidate workflow progress, then priority, then immediate incoming `depends_on` count, then recency.

That contract stays deterministic, but it does not solve the actual execution-order failure mode we keep seeing in the store: a dependency remains in `new` or `ready` while a dependent ticket has already advanced to `in-implementation` or `in-review`. In those cases the system does not automatically converge back toward prerequisite-first execution. The blocked dependent disappears from default next discovery, the prerequisite ranks low because its own state is early, and the operator has to manually inspect reverse dependencies to notice the gap.

The repository already has the building blocks for a better model. Root-scoped `ticket next <id>` and `unblocked-by <id>` can identify reverse dependents, still-blocked dependents, and remaining blocker work for a chosen prerequisite root. What is missing is a canonical redesign that lifts those dependency-convergence signals into the default next ranking and provides one shared library implementation that other surfaces can trust.

## Requested improvement

Redesign the best-next ranking contract around dependency convergence so eligible prerequisites that unblock more advanced dependents rise in the default queue, and make `ticket-api` the canonical owner of that dependency model so ranking, health, and audit surfaces all consume the same derived graph state.

## Scope

- Define a convergence-first ranking contract for default best-next discovery instead of treating graph-aware data as a late tiebreaker behind candidate state.
- Evaluate and choose deterministic graph signals such as transitive reverse-dependent reach, dependency-state gap, and maximum affected dependent state.
- Implement the shared dependency-convergence derivation in `ticket-api` so `ticket-cli`, `ticket-mcp`, ticket HTTP quality or health surfaces, and `audit-api` import one library model instead of duplicating graph traversal and state-gap logic.
- Reuse or align with the traversal model already exposed by root-scoped `ticket next <root>` and `unblocked-by` so the manual and default workflows agree.
- Specify the explanation metadata that next surfaces must expose when a lower-state prerequisite outranks a more advanced but less urgent ticket.
- Capture how audit keeps audit-specific severity and messaging while reusing the `ticket-api` model for evidence and classification.
- Keep the dedicated HTTP `GET /api/next` route as a separate consumer ticket once the ranking contract is finalized.

## Out of scope

- Implementing the dedicated HTTP route itself; that work remains tracked by `181ed793-481d-4d46-b059-0eda891365d7`.
- Leaving the shared dependency-convergence logic in CLI or audit codepaths; the redesign should remove that duplication rather than bless it.