Implement neutral shared storage/index/search symbols in memory-api with compatibility aliases.

Scope:
- add neutral API names and neutral schema/table abstractions in shared layers
- preserve legacy aliases for downstream compatibility
- ensure no functional behavior change

Acceptance criteria:
- shared layers expose neutral names and pass focused tests
- legacy aliases remain operational
- alias deprecation markers are present
