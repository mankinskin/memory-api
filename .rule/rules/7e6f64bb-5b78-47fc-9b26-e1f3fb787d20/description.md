### Domain adapters

- `rule-api` continues to own `rule-targets.yaml`, rule filters, ordered collection, duplicate detection, explain output, and generated-target bookkeeping.
- `spec-api` owns how generated content is attached to a spec folder, such as `body.md`, `sections/*.md`, or future generated artifacts.
- Neither domain should duplicate the shared snippet-rendering or newline-safe rewrite logic.