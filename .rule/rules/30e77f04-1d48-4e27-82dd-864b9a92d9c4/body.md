# Concrete workspace identifiers

`memory-api` stores (`ticket-api`, `spec-api`, `rule-api`, `doc-api`, `audit-api`) accept only concrete workspace paths. Synthetic aliases such as `default`, `..`, `~`, empty strings, or arbitrary handles are rejected with a typed error.