## Stable envelope

JSON output is always a single envelope object. The top-level keys are `payload` (on success) or `code`/`message`/`request_id` (on failure), plus an optional `request_id` echoed back at the top level. Commands never emit a bare `[ ... ]` array or a stream of newline-delimited objects.