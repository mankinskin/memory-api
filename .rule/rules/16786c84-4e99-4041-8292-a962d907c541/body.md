# Summary

`spec-api` should be able to generate spec document files from canonical snippet records the same way `rule-api` already generates markdown outputs from target configurations. The generation mechanism should not reuse `rule-api` by copying private rendering logic into `spec-api`; instead, the shared file-building path should be extracted behind a domain-agnostic abstraction.