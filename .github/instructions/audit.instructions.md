<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=2fd1e51c-820b-49df-9e91-31b747818aac slug=shared/instructions/audit/audit-instructions/l1 -->
---
description: "Use when editing or operating the audit tool. Covers CLI and MCP usage, repo config, and how to interpret audit output."
applyTo: "crates/audit-api/**,tools/cli/audit-cli/**,tools/mcp/audit-mcp/**,.audit.toml"
---

<!-- rule-api:entry id=2243078a-2ede-46dd-8268-62e3c97ebc1f slug=shared/instructions/audit/audit-guidance/purpose/l8 -->
## Purpose

`audit` is the repository quality audit tool for this workspace.

<!-- rule-api:entry id=98c0628f-6588-4dc4-9fb9-d4be7bfb3c79 slug=shared/instructions/audit/audit-guidance/purpose/l12 -->
- Core library crate: `audit-api`
- CLI package: `audit-cli` with the `audit` binary
- MCP package: `audit-mcp`
- MCP tool: `audit`

<!-- rule-api:entry id=07d051e6-7fde-419c-a1fd-e83061b1ec4c slug=shared/instructions/audit/audit-guidance/purpose/l17 -->
Keep the layering thin and explicit:

<!-- rule-api:entry id=d053641c-d80d-44f7-9deb-0cdd4ebdf829 slug=shared/instructions/audit/audit-guidance/purpose/l19 -->
1. `audit-api` owns audit logic, models, config loading, indexing, and trials.
2. `audit-cli` owns argument parsing and human/json rendering.
3. `audit-mcp` only translates MCP inputs into `audit-api` calls and serializes the result.

<!-- rule-api:entry id=ed960b95-dc41-4b6e-a803-99e70d76dcc9 slug=shared/instructions/audit/audit-guidance/purpose/l23 -->
One audit run:

<!-- rule-api:entry id=547cdd51-6e81-4fc3-96f4-292f0a21e2e5 slug=shared/instructions/audit/audit-guidance/purpose/l25 -->
1. resolves the repo root
2. loads `.audit.toml`
3. syncs source files into `.audit/audit.sqlite3`
4. prunes stale index rows not seen in the latest scan
5. collects file length, compiler warning, test success, coverage, and static complexity metrics
6. returns raw metrics plus actionable findings and deduplicated fix instructions

<!-- rule-api:entry id=e8f0616d-b639-4eb4-8acb-4b0eed13041e slug=shared/instructions/audit/audit-guidance/purpose/l32 -->
Prefer JSON output for automation and agent workflows. Prefer text output for local inspection.

<!-- rule-api:entry id=cc2ed220-69be-4ac6-bfee-b78b212a2062 slug=shared/instructions/audit/audit-guidance/cli-usage/l34 -->
## CLI Usage

Basic audit:

<!-- rule-api:entry id=c94cae90-a606-4f6e-bc56-a2fc96cf7104 slug=shared/instructions/audit/audit-guidance/cli-usage/l38 -->
```bash
cargo run -p audit-cli --bin audit -- .
```

<!-- rule-api:entry id=06e37556-0516-4c4f-91be-3ea19a20b86a slug=shared/instructions/audit/audit-guidance/cli-usage/l42 -->
Machine-readable output:

<!-- rule-api:entry id=2d083360-1fd8-4fbb-93f5-3b29064fdf04 slug=shared/instructions/audit/audit-guidance/cli-usage/l44 -->
```bash
cargo run -p audit-cli --bin audit -- --json .
```

<!-- rule-api:entry id=e0241e95-5a78-43c2-9cac-81f60c64e849 slug=shared/instructions/audit/audit-guidance/cli-usage/l48 -->
Override thresholds for a stricter audit:

<!-- rule-api:entry id=6a68252e-b548-4df6-863b-383bdb0233a4 slug=shared/instructions/audit/audit-guidance/cli-usage/l50 -->
```bash
cargo run -p audit-cli --bin audit -- . \
  --max-file-lines 300 \
  --max-cyclomatic-complexity 10 \
  --coverage-warn-below 85
```

<!-- rule-api:entry id=81883f05-fba1-444a-884a-02bc3731ea3b slug=shared/instructions/audit/audit-guidance/cli-usage/l57 -->
The default thresholds are:

<!-- rule-api:entry id=7858bf60-8f1e-4f39-bafc-fabf338f4b36 slug=shared/instructions/audit/audit-guidance/cli-usage/l59 -->
- `max_file_lines = 400`
- `max_cyclomatic_complexity = 12`
- `coverage_warn_below = 80.0`

<!-- rule-api:entry id=37bfd2e9-b32d-4cc4-a17d-53a4ce52248b slug=shared/instructions/audit/audit-guidance/mcp-usage/l63 -->
## MCP Usage

Run the server on stdio:

<!-- rule-api:entry id=0bd78f0e-3320-4c2c-8c8f-26e03758630f slug=shared/instructions/audit/audit-guidance/mcp-usage/l67 -->
```bash
cargo run -p audit-mcp --bin audit-mcp
```

<!-- rule-api:entry id=7b69751b-16cc-4097-8c8c-d84775f6508a slug=shared/instructions/audit/audit-guidance/mcp-usage/l71 -->
Tool input example:

<!-- rule-api:entry id=eadd5af7-2033-426b-ab51-c7d75d7eab52 slug=shared/instructions/audit/audit-guidance/mcp-usage/l73 -->
```json
{
  "repo_root": ".",
  "max_file_lines": 350,
  "max_cyclomatic_complexity": 10,
  "coverage_warn_below": 85.0
}
```

<!-- rule-api:entry id=9513e68c-52d7-4b30-9d19-b1cc686b08a5 slug=shared/instructions/audit/audit-guidance/mcp-usage/l82 -->
The MCP tool always returns the full structured `AuditReport` payload. Use it as the single synchronized read for repository quality state.

<!-- rule-api:entry id=184e9ac3-8b61-4d2b-a1ce-16def714557a slug=shared/instructions/audit/audit-guidance/repo-config/l84 -->
## Repo Config

`audit` auto-loads a repo-root `.audit.toml` file.

<!-- rule-api:entry id=a7c0f852-b754-436f-a09c-36395594fd50 slug=shared/instructions/audit/audit-guidance/repo-config/l88 -->
Example:
