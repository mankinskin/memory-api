## Acceptance criteria

- A shared document-builder abstraction is documented as the common generation path for snippet-backed markdown files.
- `rule-api` reuses that abstraction for markdown rendering and output preparation without changing its observable output contract.
- `spec-api` gains a generated-document capability for `body.md` and selected section files.
- The design explicitly separates store querying, duplicate detection, and target bookkeeping from generic file-building behavior.
- The first implementation keeps deterministic ordering, duplicate rejection, provenance policy, and existing newline-preservation guarantees.