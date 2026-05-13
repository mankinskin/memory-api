# Problem

The current markdown files do not provide a structured, uniform way for agents to record whether a rule entry was helpful, outdated, conflicting, or in need of revision.

# Decision Record

- Ratings are attached to rule entries as a hybrid of indexed metadata and attached assets, not as separate entities.
- Suggestions are stored as notes in feedback attached to the relevant rule entry.
- The initial rating scale should stay simple rather than pretending to be high precision.
- Automatic usage capture by tools is deferred until the basic storage, generation, and feedback loop is implemented and tested.
- Phase one supports explicit/manual feedback recording and manual session references.
- The minimum manual session-reference fields are `session_id` and `agent_or_user_id`.
- The phase-one feedback asset layout is:
  - `assets/feedback/events.ndjson` as the canonical append-only feedback log
  - indexed summary metadata on the rule entry derived from that log
- Each feedback event should minimally capture timestamp, rating, optional note text, note kind, `session_id`, and `agent_or_user_id`.
- Phase one does not add moderation states. Raw append-only capture plus later review is sufficient.

# Scope

Design and implement the first feedback layer for `rule-api`.

Minimum workflow:

- an agent identifies the rule entry it used
- the agent records a simple rating (`helpful`, `mixed`, `not-helpful`, or equivalent)
- the agent optionally adds a feedback note or suggested improvement
- the agent may attach a manual session reference with `session_id` and `agent_or_user_id`
- the system updates indexed summary metadata and preserves raw attached feedback assets
- the system can summarize recent or unresolved feedback per rule entry

# Acceptance Criteria

- A `rule-api` tool can attach a rating to a rule entry.
- A `rule-api` tool can append feedback notes and suggested improvements to a rule entry.
- A `rule-api` tool can attach a manual session reference to a rule entry using the minimum required fields.
- Feedback can be queried so low-rated entries and entries with unresolved notes are easy to find.
- Indexed feedback summary fields stay in sync with the canonical `assets/feedback/events.ndjson` log.
- Phase one explicitly excludes automatic usage capture and keeps that work out of the implementation path.

# Open Questions

None blocking phase one.