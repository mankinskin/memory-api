# Summary

Add support for generated `cargo doc` outputs so the docs family can register, describe, and serve Rust documentation HTML and rustdoc JSON artifacts.

## Acceptance Criteria

- The docs family can discover generated `cargo doc` HTML and rustdoc JSON outputs for workspace packages and targets.
- Stored metadata maps packages and targets to generated HTML and JSON artifact locations.
- Focused validation covers multi-package workspaces, target selection, and missing artifact handling.
