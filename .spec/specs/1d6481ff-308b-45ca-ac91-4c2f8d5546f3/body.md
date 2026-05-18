# spec-cli

Bootstrapped from source analysis.

## Create Command

`spec create --root` does not place the spec folder at the literal path the
caller passes.

The create flow delegates target-root normalization to `SpecStore::create`, so
workspace roots, the local `.spec` store root, and paths inside that store all
create under `.spec/specs`.

Targets outside the registered scan roots and outside the local `.spec` store
must be rejected.

See child specs for individual module documentation.
