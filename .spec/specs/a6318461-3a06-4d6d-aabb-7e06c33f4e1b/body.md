# Summary

`audit-api` should flag orphan tickets so every active ticket participates
in the repository's `depends_on` graph.

## Required behavior

- When `audit run` targets a repository with a local `.ticket` store, the
	audit loads tickets and edges through `ticket-api` and evaluates ticket
	topology as an additional trial.
- Only `depends_on` edges satisfy this rule. A ticket is considered linked when
	it has at least one outgoing `depends_on` dependency or at least one incoming
	dependee.
- Any active ticket with neither dependencies nor dependees produces an
	audit finding with the ticket folder path, ticket id, state, and the observed
	edge counts.
- Findings must guide the user toward either linking the ticket to its real
	prerequisites or creating a project-tracker parent ticket that depends on the
	otherwise standalone task.
- Repositories without a local `.ticket` store must not hard-fail the entire
	audit. The ticket-topology metric should report as unavailable instead.

## Acceptance criteria

- A repository with one orphan ticket and one linked ticket pair reports a
	single orphan-ticket finding.
- A repository where every ticket participates in at least one `depends_on`
	relationship reports zero orphan-ticket findings.
- A repository without a `.ticket` store keeps the audit runnable and marks the
	ticket-topology metric unavailable.
- The public audit spec records ticket-topology validation as part of the audit
	trial set.

## Traceability

- Tracking ticket: `.ticket/tickets/a762448e-464c-43da-95b8-e49eb07814ed`
- Implementation files:
	- `crates/audit-api/src/trials/ticket_graph.rs`
	- `crates/audit-api/src/audit.rs`
	- `crates/audit-api/src/models.rs`
	- `crates/audit-api/Cargo.toml`
- Related spec update: `.spec/specs/0c3f11d3-2475-470c-a191-beedd2c8e53c/body.md`

## Validation

- `cargo test -p audit-api ticket_graph` — passed
- `spec refs a6318461-3a06-4d6d-aabb-7e06c33f4e1b validate --index-root .spec --json` — passed (`valid: true`)
- `spec health a6318461-3a06-4d6d-aabb-7e06c33f4e1b --index-root .spec --json` — passed (`issues_count: 0`)
