# spec-cli: Command-Line Interface

## Objective

Create a `spec` CLI binary with CRUD, search, hierarchy navigation, and health check commands.

## Commands

```
spec create --title "..." --slug "..." --component "..." [--parent <id|slug>] [--scope module] [--body-file body.md]
spec get <id|slug> [--full]  # --full includes sections
spec update <id|slug> --field key=value [--state <state>]
spec delete <id|slug>
spec list [--where field=value] [--limit N]
spec search <query>
spec health [--all | <id|slug>]
spec tree [<id|slug>]  # show hierarchy as tree
spec refs <id|slug>  # list code references
spec refs validate <id|slug>  # check reference integrity
spec section add <spec-id|slug> --name <name> --file <path>
spec section list <spec-id|slug>
spec section get <spec-id|slug> <name>
spec scan [--force]
spec toc  # print table of contents
spec skill generate [--crate <name>] [--domain <name>] [--all]
```

## Crate Structure

```
tools/cli/spec-cli/
├── Cargo.toml
└── src/
    ├── bin/spec.rs
    ├── lib.rs
    └── cli/
        ├── args.rs
        ├── dispatch.rs
        └── commands/
            ├── crud.rs
            ├── query.rs
            ├── hierarchy.rs
            ├── refs.rs
            ├── sections.rs
            └── skill.rs
```

## Acceptance Criteria

- [ ] `spec` binary compiles and runs
- [ ] CRUD commands working with slug resolution
- [ ] `spec tree` displays parent-child hierarchy
- [ ] `spec search` uses Tantivy full-text search
- [ ] `spec health` validates completeness and references
- [ ] `spec skill generate` produces SKILL.md files
- [ ] JSON output mode with `--json` flag