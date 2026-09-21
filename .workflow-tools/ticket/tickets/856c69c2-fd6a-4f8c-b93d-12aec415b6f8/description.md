# T6: `pdf-cli` Transport

## Objective

Wire the `pdf-cli` binary so every `pdf-api` operation is reachable from the
command line. The CLI is a thin adapter — no PDF logic lives here.

## Files

- `memory-api/crates/pdf/src/bin/pdf-cli.rs` (replace T1 stub)
- `memory-api/crates/pdf/tests/cli.rs` (new)

## Design

Use `transport-harness` for shared CLI scaffolding, gated by the `cli` feature.
`memory-api/crates/ticket/src/bin/ticket-cli.rs` is the in-repo reference for
how the harness is consumed; follow it rather than hand-rolling arg parsing
conventions.

### Subcommands

One per `pdf-api` operation:

```
pdf-cli extract-text <input> [--pages <range>] [--max-chars <n>]
pdf-cli merge <inputs...> --output <path> [--overwrite]
pdf-cli split <input> --output-dir <dir> [--pages <ranges>] [--overwrite]
pdf-cli edit-pages <input> --output <path> [--delete <r>] [--reorder <spec>] [--rotate <spec>] [--overwrite]
pdf-cli metadata get <input>
pdf-cli metadata set <input> --output <path> [--title ...] [--author ...] [--overwrite]
pdf-cli create --mode <programmatic|typst> ... --output <path> [--overwrite]
pdf-cli extract-images <input> --output-dir <dir> [--overwrite]   # gated on T9
```

### Sandbox root

Every invocation needs a root. Provide `--root <path>` with a documented default
(current working directory is the reasonable default for a CLI). Document
clearly that the root is a safety boundary, not a convenience.

### Output format

Support `--json` and, per the repo's compact-output guidance, `--toon` for
machine-readable output, with human-readable as the default. `peek-api` already
depends on `toon-format`; follow that precedent rather than hand-rolling.

### Exit codes

- 0 success.
- Distinct non-zero code for user errors vs internal errors, driven by
  `PdfError::is_user_error()` from T2. Errors go to stderr; machine-readable
  output stays clean on stdout.

## Acceptance Criteria

- [ ] Every `pdf-api` operation has a subcommand.
- [ ] `--help` documents every subcommand and flag, including that `--root` is a
      security boundary.
- [ ] `--json` and `--toon` produce valid parseable output; human output is the
      default.
- [ ] User errors exit with a distinct code from internal errors.
- [ ] Errors go to stderr; stdout stays clean in machine-readable modes.
- [ ] `--overwrite` is required to clobber; without it the CLI refuses and says
      so.
- [ ] A path outside `--root` is refused.
- [ ] The binary only builds under `--features cli`.
- [ ] No PDF logic in the CLI — it constructs a `PdfRequest` and calls
      `execute()`.

## Validation

```bash
cargo test -p pdf --features cli
cargo run -p pdf --features cli --bin pdf-cli -- --help
```

## Depends On

T3, T4, T5.
