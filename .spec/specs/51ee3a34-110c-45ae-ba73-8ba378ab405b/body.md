# Summary
Require that every spec is introduced and explained in-session by a governing PolicyRule, with presentation conditioned on the spec's implementation status. Wires into session construction (epic effba966) so spec availability is legible and policy coverage is forced for specced features.

# Motivation (why)
Specs today are invisible at runtime — nothing guarantees an agent is told a spec exists, whether it is implemented, or how to use it idiomatically. This lets agents assume unavailable behavior and lets specced features ship without governing policy. Requiring a rule to introduce each spec closes both gaps.

# Status-conditioned presentation
- implemented — rule presents the spec as a live contract dependents can rely on.
- partial-with-gaps — rule presents the spec with explicit awareness of unimplemented positions (from spec-contract v2 positions).
- coming-soon / not-implemented — rule shows a "coming soon" note so agents do not assume availability.

# Provided Surface Contracts
- Obligation: each spec has >= 1 governing rule that introduces it in session construction.
- The rule's presentation branch is selected from the spec's computed implementation status.

# Required Validation
- Coverage check: a specced feature without a governing rule is flagged (feeds missing-rule ticketing, G-D).
- Status-branch check: presentation matches spec position status for implemented / partial / coming-soon.

# Related Implementation Tickets
- Ticket 6875dff6 (G-C) under epic 3be95a71.
- Consumes spec-contract v2 positions (memory-api/spec-api/spec-contract-v2).
- Presentation delivered through session construction (effba966 cascade/pin).

# Background Knowledge References
- Rule-URN shape + session_context schema frozen in spec 8c880efc ADR sections.