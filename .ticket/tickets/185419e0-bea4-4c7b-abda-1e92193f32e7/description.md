# Problem

Ticket state transitions should work in both directions by default using the same state transition interface. We should not require every schema to spell out reverse edges when the validator can treat declared transitions as bidirectional.

# Scope

Make transition validation and path resolution treat each declared state edge as usable in both directions unless a schema explicitly says otherwise.

# Acceptance Criteria

- The ticket state machine accepts reverse movement through the existing update path without adding reverse edges to the schema.
- Transition validation still rejects states that are not connected by a declared transition.
- Focused ticket-api regression coverage proves at least one backward transition through the public store API.