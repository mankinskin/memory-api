# Output

`audit` returns an `AuditReport` with these top-level fields:

- `service`
- `repo_root`
- `index_database`
- `sync`
- `run`
- `metrics`
- `findings`
- `instructions`

`audit summary --by ...` and `audit_summary` return an `AuditSummaryReport` with these top-level fields:

- `repo_root`
- `by`
- `total_findings`
- `repo_wide_issues`
- `groups`
- `unmapped_paths`

## Sync semantics

`sync.pruned_files` is the indicator that stale index rows were removed during this run. `updated_files` and `reused_files` distinguish newly refreshed content from unchanged index entries.

## Metric semantics

Each trial returns a status:

- `collected`
- `unavailable`
- `failed`
- `not_applicable`

Unavailable metrics remain part of the report. For example, missing `cargo llvm-cov` should produce an unavailable coverage summary rather than aborting the audit.

## Findings

Each finding carries:

- severity
- summary
- optional path and line
- raw metric value and threshold
- concrete repair instructions
- structured evidence for downstream tooling

`instructions` at the report root is the deduplicated union of all finding-level fix guidance.

## Human-readable projection

Text output from the full `audit` report shows:

- repo path and index path
- sync summary
- metric summaries
- one finding line per issue
- `fix:` lines under each finding

Text output from the summary view shows:

- repo path
- grouping key
- total findings and repo-wide issues
- one grouped count line per key
- unmapped paths when crate ownership cannot be resolved

Each text view is a condensed rendering of the same JSON contract used in automation mode.

The transport split is intentional:

- `audit-api` produces the `AuditReport`
- `audit-api` also produces the `AuditSummaryReport`
- `audit-cli` renders both contracts for terminal use
- `audit-mcp` serializes both contracts for MCP clients