# Problem

Agents can now attach ratings and notes to canonical rule entries, but they still cannot attach feedback directly to native `spec-api` entities. Today the only supported workaround is to resolve a generated rule entry and encode the spec ID, path, and section inside the note text.

# Decision Record

- Direct feedback on native spec entities is a follow-up to rule-entry feedback, not part of the completed phase-one `rule-api` slice.
- The follow-up should preserve the same basic feedback shape: rating, optional note text, optional note kind, and optional manual session references.
- Public spec surfaces should expose this directly instead of requiring agents to translate spec feedback into rule-entry feedback notes.
- Integration tests are required for the public workflow, not just unit tests.

# Scope

Design and implement direct feedback support for native `spec-api` entities.

Minimum workflow:

- an agent identifies a spec entity it used
- the agent records a simple rating (`helpful`, `mixed`, `not-helpful`)
- the agent optionally adds a note or suggested improvement
- the agent may attach a manual session reference with `session_id` and `agent_or_user_id`
- the system updates indexed summary metadata and preserves canonical feedback assets for the spec entity
- the public spec tooling can query low-rated entities and entities with unresolved notes directly

# Acceptance Criteria

- A public `spec-api` tool can attach a rating directly to a spec entity.
- A public `spec-api` tool can append feedback notes and suggested improvements directly to a spec entity.
- A public `spec-api` tool can attach a manual session reference using `session_id` and `agent_or_user_id`.
- Feedback for spec entities can be queried so low-rated entities and entities with unresolved notes are easy to find.
- Indexed feedback summary fields stay in sync with the canonical spec feedback asset log.
- Add integration tests that exercise at least one public spec surface end to end for feedback recording and querying.
- Update generated agent/tool instructions if the spec-feedback workflow becomes directly available to agents.

# Open Questions

- Reuse the exact rule feedback asset layout for specs unless a store-specific constraint forces a different path.
- Prefer matching filter names and rating values across rule and spec surfaces unless there is a compatibility reason not to.