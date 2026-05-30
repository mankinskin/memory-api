## Non-duplication rule

Adapters must not re-implement model behaviour. If a CLI or MCP tool needs to enforce a precondition, the precondition lives in `<x>-api` and the adapter calls it. New cross-cutting features are added to `<x>-api` first, then exposed by the adapters in lockstep.