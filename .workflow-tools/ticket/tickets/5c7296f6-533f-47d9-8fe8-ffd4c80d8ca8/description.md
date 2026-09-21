# Session Objective
Split remaining **cross-cutting tooling/matrix** file_length offenders to close out the tail of batch-1.

# Scope
- memory-api/crates/memory-matrix/src/mcp.rs (868)
- memory-api/tools/cli/spec-cli/src/cli/dispatch_tests.rs (833)
- memory-api/tools/cli/rule-cli/src/cli/dispatch.rs (785)

# Acceptance Criteria
- Keep semantics unchanged and avoid mixed-concern rewrites.
- Prefer test/helper extraction before structural refactors.
- Run targeted matrix/spec-cli/rule-cli validations.
