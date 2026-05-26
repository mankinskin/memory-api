## Failure modes

- Inputs shorter than the minimum prefix length are rejected with `code: invalid_request`.
- Ambiguous prefixes are rejected with `code: conflict` and `details` listing the matching ids so callers can disambiguate.
- The resolver is read-only; it never mutates the store and never creates entities on miss.