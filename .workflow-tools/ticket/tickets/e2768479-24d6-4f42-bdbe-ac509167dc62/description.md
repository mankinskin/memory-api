Migrate rule-api and spec-api to consume neutral memory-api shared APIs.

Scope:
- replace legacy ticket-biased calls in rule-api/spec-api
- keep behavior parity for scan/create/update/search flows
- remove unnecessary compatibility dependence in these domains

Acceptance criteria:
- rule-api/spec-api compile and tests pass against neutral API surface
- no direct use of ticket-biased shared symbols remains in rule/spec crates
