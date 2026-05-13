# Interview: Write API Authentication

**Date:** 2026-04-08
**Applies to:** `69abd1c7` (CRUD mutations), `15871ee6` (Edge mutations), `3fda11c3` (History/revert), `189a6068` (Schema endpoint)

## Question

ticket-http is currently read-only with no auth. Adding write endpoints raises the question:

- Is this strictly local/localhost? If so, no auth needed.
- If it could be network-exposed: do you want token-based auth, session cookies, or something else?
- Should write endpoints require a lease to prevent concurrent edits?

## Answer

**Could become multi-user or network exposed later on. Reuse existing auth backend from viewer-api backends (most mature).**

## Implications

- Port the auth middleware from viewer-api HTTP backends to ticket-http
- Research viewer-api's existing auth implementation for token format, session handling, and middleware pattern
- Design write endpoints to integrate with the same auth backend from day one
- Multi-user capability means lease-based conflict prevention becomes important
- All Track 3 endpoints should go through the auth middleware
- Auth should be optional (disabled for localhost dev mode, enabled for network mode)
