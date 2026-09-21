# Session Objective
Split remaining **rule + fixture** file_length offenders with narrow helper/test-module extraction.

# Scope
- memory-api/crates/memory-fixtures/src/lib.rs (929)
- memory-api/crates/rule-api/src/targets.rs (913)
- memory-api/crates/rule-api/src/targets/tests/tests_load.rs (865)

# Acceptance Criteria
- Keep behavior unchanged and avoid broad refactors.
- Preserve existing test naming and module boundaries.
- Run focused rule-api/memory-fixtures validation after edits.
