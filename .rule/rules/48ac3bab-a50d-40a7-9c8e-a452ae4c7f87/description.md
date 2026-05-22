## Feedback Workflow

- Record feedback on canonical rule entries today. `spec-api` entries do not yet expose direct feedback tools or feedback summary fields.
- When feedback came from a specific generated spec or instruction surface, first locate the canonical rule entry that produced that text. Use `rule search` or `rule_list` / `rule_search` with `repo_scope`, `path_scope`, `section`, or `slug` filters until you have the exact rule ID or slug.
- Record the feedback on that rule entry with either:
  - CLI: `rule feedback <id-or-slug> --rating helpful|mixed|not-helpful [--note "..."] [--note-kind note|suggestion] [--session-id <id> --agent-or-user-id <id>]`
  - MCP: `rule_record_feedback` with `id`, `rating`, optional `note`, optional `note_kind`, and the `session_id` + `agent_or_user_id` pair when a manual session reference is needed.
- If you are reacting to a native spec entry rather than generated rule text, include the spec ID, path, and section in the feedback note text and open or update the corresponding spec or ticket work. Do not describe that as direct spec-entry feedback when the current storage is rule-entry scoped.
- Review follow-up queues with `rule list --low-rated-only`, `rule list --unresolved-only`, or the MCP `rule_list` / `rule_search` flags `low_rated_only` and `unresolved_only`.