# code_ref

Source: `crates/spec-api/src/code_ref.rs`

## Public API

### `SymbolKind` (Enum)

The kind of symbol a code reference points to.

### `CodeRef` (Struct)

A reference from a spec to a specific symbol in implementation code.

### `RefValidation` (Struct)

Validation result for a single code ref.

### `validate_refs` (Function)

Validate code refs against a workspace root.

### `find_refs_for_file` (Function)

Reverse lookup: find which code refs reference a given file path.

