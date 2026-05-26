<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=fba0b27a-3856-42f7-88ae-f2abd053cae9 slug=spec-api/generated-documents/migration-workflow/l1 -->
## Migration workflow

This pilot keeps authored `spec.toml` metadata local while canonical prose lives in `spec-doc` rules.

1. Update canonical prose in the matching `spec-doc` rules, or bootstrap a draft with `rule import-file`.
2. Keep `.spec/specs/1cf68c36-7f64-4d81-b553-1947b978fbe3/generated.toml` as the artifact-to-target map.
3. Run `spec sync-generated 1cf68c36-7f64-4d81-b553-1947b978fbe3` to rewrite the generated artifacts through `spec-api`.
4. Revalidate with `spec refs 1cf68c36-7f64-4d81-b553-1947b978fbe3 validate --workspace-root .`.
