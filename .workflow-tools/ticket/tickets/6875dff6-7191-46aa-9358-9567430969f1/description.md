## G-C — Rule-introduces-spec policy

Require that every spec is introduced/explained in-session by a governing PolicyRule, conditioned on implementation status:

- **implemented** — rule presents the spec as a live contract dependents can rely on.
- **partial-with-gaps** — rule presents the spec with explicit awareness of unimplemented positions.
- **coming-soon / not-implemented** — rule shows a "coming soon" note so agents don't assume availability.

This forces agents to actually author policy for specced features, and makes spec availability legible during session construction (consumed by effba966 cascade/pin).

## Deliverables
- A spec (memory-api) defining the rule-introduces-spec obligation and its status conditioning.
- A workflow policy rule wiring the obligation into session construction.

## Acceptance criteria
- The obligation is specced and tied to a rule.
- Status-conditioned presentation (implemented / partial / coming-soon) is defined and testable.