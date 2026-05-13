# Problem

We need a concrete `rule-api` domain on top of `memory-api` storage primitives so canonical rule entries can be stored, indexed, searched, versioned, rendered into markdown, and annotated with ratings and feedback.

# Decision Record

- `rule-api` is the new shared domain API for user-facing instruction/rule content.
- `memory-api` provides the common storage integration beneath `rule-api`, just as it does for `ticket-api`, `spec-api`, and `audit-api`.
- Paragraphs are the canonical entry granularity.
- Sentence-level references are optional anchors inside a paragraph, not separate first-class entities in phase one.
- Rule entries use the repo's existing UUID-plus-slug pattern:
  - `id` in the manifest is the immutable primary UUID
  - `slug` is a required semantic key stored in manifest metadata
  - CLI, MCP, and provenance comments should carry both where practical
- Ratings, feedback notes, and session references remain attached to a rule entry as a hybrid of indexed metadata plus attached assets.
- Manual session references in phase one require at least `session_id` and `agent_or_user_id`.
- Rule-entry lifecycle states are schema-enforced.
- Phase-one indexed feedback summary fields are:
  - `feedback_helpful_count`
  - `feedback_mixed_count`
  - `feedback_not_helpful_count`
  - `feedback_note_count`
  - `feedback_unresolved_count`
  - `feedback_last_at`

# Scope

Design and implement the `rule-api` data model and storage contract.

Candidate rule entry fields:

- primary UUID `id`
- required semantic `slug`
- canonical paragraph body
- optional sentence anchors
- target file kind (`AGENTS`, `.github/README`, `.instructions`, `.prompt`, `.agent`)
- logical section id
- ordering key within file/section
- repo applicability and path filters
- provenance fields for imported content
- schema-enforced lifecycle state
- indexed feedback summary fields
- attached feedback/session assets

# Acceptance Criteria

- A `rule-api` storage layout is defined using existing `memory-api` manifest/body/history/search/index patterns.
- Stable identity rules are documented for both migrated and newly authored entries using UUID plus required slug.
- The model cleanly supports rendering rule entries into files, including ordering, sectioning, and repo applicability.
- The model cleanly supports attached ratings, feedback notes, and manual session references without introducing separate feedback entity types.
- Search/index fields support filtering by repo, file kind, section, status, stable rule id, rating summary, and unresolved feedback state.
- Lifecycle states are enforced through schema rather than convention alone.

# Open Questions

None blocking phase one. If index pressure appears during implementation, additional feedback summary fields can stay derived instead of indexed.