## Rejected inputs

- The literal string `default` (legacy alias).
- `.` or `..` resolved at the call site when those expand outside any registered scan root.
- Paths that do not resolve to a checkout containing a `.ticket/`, `.spec/`, or `.rule/` directory after walking up to the nearest registered root.
- In move operations, any source or target identifier resolving to the exact same workspace root is rejected. Moves must be between distinct concrete stores.