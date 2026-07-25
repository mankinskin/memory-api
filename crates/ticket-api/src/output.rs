use serde_json::Value;

use crate::workspace::DEFAULT_WORKSPACE_NAME;

/// Remove default-identifying metadata from serialized ticket outputs.
///
/// The default workspace (`default`) is implied across the ticket surfaces, so
/// it is omitted from machine-readable payloads unless a non-default value is
/// present. ticket-api makes no assumption about a "default" ticket type, so
/// the `type` field is never stripped.
pub fn strip_default_metadata(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for child in map.values_mut() {
                strip_default_metadata(child);
            }

            if matches!(map.get("workspace"), Some(Value::String(workspace)) if workspace == DEFAULT_WORKSPACE_NAME)
            {
                map.remove("workspace");
            }
        },
        Value::Array(items) =>
            for item in items {
                strip_default_metadata(item);
            },
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::strip_default_metadata;

    #[test]
    fn strips_default_workspace_but_retains_type() {
        let mut value = json!({
            "workspace": "default",
            "items": [
                {
                    "id": "abc",
                    "type": "tracker-improvement"
                }
            ],
            "ticket": {
                "fields": {
                    "title": "hello",
                    "type": "tracker-improvement"
                }
            }
        });

        strip_default_metadata(&mut value);

        assert!(value.get("workspace").is_none());
        // No assumption of a default type: the type field is preserved.
        assert_eq!(value["items"][0]["type"], "tracker-improvement");
        assert_eq!(value["ticket"]["fields"]["type"], "tracker-improvement");
    }

    #[test]
    fn retains_non_default_workspace_and_schema() {
        let mut value = json!({
            "workspace": "alternate",
            "type": "feature"
        });

        strip_default_metadata(&mut value);

        assert_eq!(value["workspace"], "alternate");
        assert_eq!(value["type"], "feature");
    }
}
