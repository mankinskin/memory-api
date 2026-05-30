# <x>-api crate owns the model

For each store domain (`ticket-api`, `spec-api`, `rule-api`, `doc-api`, `audit-api`, `mem-api`), the `<x>-api` crate owns the canonical model: entity types, store traits, validation, indexing, history, and the in-process API. CLI, MCP, and HTTP tools are thin adapters that translate transport into `<x>-api` calls.