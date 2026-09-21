# T5: PDF Creation — Programmatic Primitive + Optional Typst Path

## Objective

Implement the `Create` variant with two modes, per locked decision 4.

## Files

- `memory-api/crates/pdf-api/src/create.rs` (new)
- `memory-api/crates/pdf-api/src/typst.rs` (new — detection + invocation)
- `memory-api/crates/pdf-api/src/lib.rs` (wire into `execute()`)
- `memory-api/crates/pdf-api/tests/create.rs` (new)

## Design

### Mode A — programmatic primitive (always available)

**Backend: `krilla` (decided, do not re-open).** `printpdf` was the original
assumption and has been dropped. Rationale from the research report:

- `krilla` 0.8.2, `MIT OR Apache-2.0`, actively maintained (last push within
  days of the decision), built on `pdf-writer` from the Typst org.
- Pure Rust throughout — no `*-sys`, no C/C++ deps anywhere in its tree.
- Real font-subsetting pipeline (`skrifa` / `subsetter` / `rustybuzz`), which is
  a materially better text story than `printpdf`, whose only documented path
  requires `include_bytes!`-embedding a TTF/OTF with no apparent base-14/no-embed
  alternative.
- Optional pure-Rust image codecs (`png`, `gif`, `zune-jpeg`, `image-webp`) if
  image embedding is ever wanted later.

If implementation reveals `krilla` cannot deliver the Mode A scope below, that is
a **blocker to escalate**, not a licence to silently swap back to `printpdf`.

Deliberately minimal: this is a primitive for agents that need to emit a simple
document, not a layout engine.

Scope it to: page size, pages, text blocks with a basic font/size, and simple
line breaks. Explicitly out of scope: tables, images, columns, styling systems.
If a caller needs real typesetting, they should use Mode B.

Font handling is the main trap. **T0 confirmed (reading krilla's public API)
that every text-drawing call requires an explicit caller-supplied `Font`,
constructed via `Font::new(font_bytes, ...)`. There is no default/base-14 font
and no no-embed text path — a font asset MUST be vendored, not "if needed".**
krilla itself vendors `NotoSans-Regular.ttf` (OFL-licensed) as its own
test/example asset — a reasonable candidate. Select a permissively licensed
open font (OFL or Apache-2.0), vendor it into the crate, and record the licence
and its provenance in the crate README. Non-ASCII text must either work or fail
with a clear error — never silently emit garbage or notdef glyphs.

### Mode B — typst (optional, external process)

**Verified invocation contract** (read directly from `typst/typst` source,
`crates/typst-cli/src/args.rs` and `main.rs`, version 0.15.1, `Apache-2.0`).
Use these facts rather than re-deriving them:

| Need | Flag / behavior |
|---|---|
| Compile | `typst compile <input> [output]` |
| Root confinement | `--root <DIR>` (also `TYPST_ROOT` env) — first-class flag |
| Font confinement | `--font-path <DIR>` (repeatable, `TYPST_FONT_PATHS`) |
| Ambient font isolation | `--ignore-system-fonts` |
| Stdin source | input `-` |
| Stdout output | output `-`, or omitted |
| Exit codes | **binary only** — `SUCCESS` / `FAILURE`, no error-class taxonomy |

Consequence for error handling: do **not** assume a rich exit-code taxonomy.
Classification must come from parsing stderr, or from accepting binary
success/failure and surfacing stderr verbatim in the error.

Use `--ignore-system-fonts` together with an explicit `--font-path` inside the
sandbox so output is reproducible and does not depend on ambient system fonts.

- Input is typst markup (source string or a source file inside the sandbox
  root).
- `typst-cli` is invoked as an **external process**. It is NOT a build
  dependency and NOT a linked library.
- Detect it on `PATH` at call time.
- If absent, return `PdfError::ToolUnavailable` with an actionable message
  naming the binary and how to install it. Never panic. Never silently fall back
  to Mode A — a silent downgrade would produce a document that looks nothing
  like what was asked for.

**Security — this is the sharp edge of the whole track.** Invoking an external
process with agent-supplied input requires care:

- Pass the source via a temp file inside the sandbox root, not via a shell
  string.
- Invoke the binary directly with argument vectors; never through a shell.
- Do not let caller input inject additional CLI flags — anything caller-supplied
  goes in a position where it cannot be parsed as a flag, or is validated
  against an allowlist.
- Constrain typst's own root/input access so markup cannot read arbitrary files
  via typst's include/read features and exfiltrate them into the output PDF.
- Apply a wall-clock timeout so a pathological document cannot hang the caller
  indefinitely.
- Cap output size.
- The output path is still subject to the T2 write policy.

### Mode selection

The request carries an explicit mode. Do not auto-detect from content —
ambiguity here becomes a silent-wrong-output bug.

## Acceptance Criteria

- [ ] Mode A produces a valid, openable PDF with the requested pages and text.
- [ ] Mode A is implemented on `krilla`; `printpdf` appears nowhere in the
      dependency tree.
- [ ] Mode A's font/encoding behavior is documented; non-ASCII either works or
      errors clearly.
- [ ] If a font asset is vendored, its licence and provenance are recorded in
      the crate README and the licence is OFL/Apache-2.0/MIT-compatible.
- [ ] Mode B produces a valid PDF when `typst-cli` is present.
- [ ] Mode B returns `ToolUnavailable` with an actionable message when
      `typst-cli` is absent — verified by a test that runs with a `PATH` that
      excludes it.
- [ ] Mode B never silently falls back to Mode A.
- [ ] typst is invoked without a shell; a source containing shell metacharacters
      and flag-like tokens cannot alter the invocation — asserted by test.
- [ ] typst markup cannot read files outside the sandbox root — asserted by a
      test that attempts it.
- [ ] `--ignore-system-fonts` plus an in-sandbox `--font-path` is used, so
      output does not depend on ambient system fonts.
- [ ] typst stderr is surfaced in the error message, since exit codes are binary
      and carry no error-class information.
- [ ] A timeout aborts a runaway typst invocation and returns a clean error.
- [ ] typst output beyond the configured size cap is truncated and the
      truncation is reported rather than silently swallowed — asserted by a test
      using a pathological document.
- [ ] Output path honors T2 copy-on-write / `overwrite` policy.
- [ ] Tests that require `typst-cli` skip cleanly (not fail) when it is absent,
      so CI without typst stays green.

## Validation

```bash
cargo test -p pdf-api create
```

Record whether `typst-cli` was present in the validating run, since that changes
which tests actually executed.

## Depends On

T2.
