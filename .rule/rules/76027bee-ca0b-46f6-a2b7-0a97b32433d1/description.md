# Shared id/prefix resolver

All `memory-api` stores share one id-or-prefix resolver. Whether the caller is `ticket-cli`, `spec-mcp`, `rule-http`, or a viewer, the rules for accepting `<full-uuid>` versus `<8-char-prefix>` versus a slug are identical.