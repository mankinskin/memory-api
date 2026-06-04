use serde_json::Value;

use crate::{
    model::default_schema::TYPE_ID,
    workspace::DEFAULT_WORKSPACE_NAME,
};

/// Remove default-identifying metadata from serialized ticket outputs.
///
/// The default workspace (`default`) and default ticket schema
/// (`tracker-improvement`) are implied across the ticket surfaces, so they are
/// omitted from machine-readable payloads unless a non-default value is
/// present.
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

            if matches!(map.get("type"), Some(Value::String(type_id)) if type_id == TYPE_ID)
            {
                map.remove("type");
            }

            if matches!(map.get("type_id"), Some(Value::String(type_id)) if type_id == TYPE_ID)
            {
                map.remove("type_id");
            }
        },
        Value::Array(items) => {
            for item in items {
                strip_default_metadata(item);
            }
        },
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::strip_default_metadata;

    #[test]
    fn strips_default_workspace_and_schema_recursively() {
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
        assert!(value["items"][0].get("type").is_none());
        assert!(value["ticket"]["fields"].get("type").is_none());
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