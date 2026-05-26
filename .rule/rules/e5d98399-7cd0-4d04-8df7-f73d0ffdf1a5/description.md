# Required-states one-way state machine

Ticket and spec types declare their state machine through `required_states` in the type schema. Transitions are one-way (no automatic backward moves) and are gated by the schema, not by ad-hoc CLI/MCP logic.