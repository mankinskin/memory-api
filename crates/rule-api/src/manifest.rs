use std::collections::BTreeMap;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    Number,
    Value,
};
use uuid::Uuid;

pub type RuleId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleState {
    Draft,
    Reviewed,
    Adopted,
    Deprecated,
}

impl RuleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Reviewed => "reviewed",
            Self::Adopted => "adopted",
            Self::Deprecated => "deprecated",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleManifest {
    pub id: RuleId,
    pub created_at: DateTime<Utc>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl RuleManifest {
    pub fn new(
        slug: &str,
        title: &str,
        file_kind: &str,
        section: &str,
        body: &str,
    ) -> Self {
        let mut extra = BTreeMap::new();
        extra.insert("slug".to_string(), Value::String(slug.to_string()));
        extra.insert("title".to_string(), Value::String(title.to_string()));
        extra.insert(
            "type".to_string(),
            Value::String("rule-entry".to_string()),
        );
        extra.insert(
            "state".to_string(),
            Value::String(RuleState::Draft.as_str().to_string()),
        );
        extra.insert(
            "file_kind".to_string(),
            Value::String(file_kind.to_string()),
        );
        extra.insert("section".to_string(), Value::String(section.to_string()));
        extra.insert("body".to_string(), Value::String(body.to_string()));
        extra.insert("order_key".to_string(), Value::Number(0.into()));
        extra.insert("repo_scopes".to_string(), Value::Array(Vec::new()));
        extra.insert("path_scopes".to_string(), Value::Array(Vec::new()));
        extra.insert("sentence_anchors".to_string(), Value::Array(Vec::new()));
        extra.insert(
            "feedback_helpful_count".to_string(),
            Value::Number(0.into()),
        );
        extra.insert(
            "feedback_mixed_count".to_string(),
            Value::Number(0.into()),
        );
        extra.insert(
            "feedback_not_helpful_count".to_string(),
            Value::Number(0.into()),
        );
        extra
            .insert("feedback_note_count".to_string(), Value::Number(0.into()));
        extra.insert(
            "feedback_unresolved_count".to_string(),
            Value::Number(0.into()),
        );

        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            extra,
        }
    }

    pub fn slug(&self) -> Option<&str> {
        self.extra.get("slug").and_then(|value| value.as_str())
    }

    pub fn title(&self) -> Option<&str> {
        self.extra.get("title").and_then(|value| value.as_str())
    }

    pub fn state(&self) -> Option<&str> {
        self.extra.get("state").and_then(|value| value.as_str())
    }

    pub fn file_kind(&self) -> Option<&str> {
        self.extra.get("file_kind").and_then(|value| value.as_str())
    }

    pub fn section(&self) -> Option<&str> {
        self.extra.get("section").and_then(|value| value.as_str())
    }

    pub fn body(&self) -> Option<&str> {
        self.extra.get("body").and_then(|value| value.as_str())
    }

    pub fn order_key(&self) -> Option<i64> {
        self.extra.get("order_key").and_then(Value::as_i64)
    }

    pub fn repo_scopes(&self) -> Vec<String> {
        string_array(&self.extra, "repo_scopes")
    }

    pub fn path_scopes(&self) -> Vec<String> {
        string_array(&self.extra, "path_scopes")
    }

    pub fn sentence_anchors(&self) -> Vec<String> {
        string_array(&self.extra, "sentence_anchors")
    }

    pub fn source_repo(&self) -> Option<&str> {
        self.extra.get("source_repo").and_then(Value::as_str)
    }

    pub fn source_path(&self) -> Option<&str> {
        self.extra.get("source_path").and_then(Value::as_str)
    }

    pub fn source_start_line(&self) -> Option<i64> {
        self.extra.get("source_start_line").and_then(Value::as_i64)
    }

    pub fn source_end_line(&self) -> Option<i64> {
        self.extra.get("source_end_line").and_then(Value::as_i64)
    }

    pub fn feedback_helpful_count(&self) -> Option<i64> {
        self.extra
            .get("feedback_helpful_count")
            .and_then(Value::as_i64)
    }

    pub fn feedback_mixed_count(&self) -> Option<i64> {
        self.extra
            .get("feedback_mixed_count")
            .and_then(Value::as_i64)
    }

    pub fn feedback_not_helpful_count(&self) -> Option<i64> {
        self.extra
            .get("feedback_not_helpful_count")
            .and_then(Value::as_i64)
    }

    pub fn feedback_note_count(&self) -> Option<i64> {
        self.extra
            .get("feedback_note_count")
            .and_then(Value::as_i64)
    }

    pub fn feedback_unresolved_count(&self) -> Option<i64> {
        self.extra
            .get("feedback_unresolved_count")
            .and_then(Value::as_i64)
    }

    pub fn feedback_last_at(&self) -> Option<&str> {
        self.extra.get("feedback_last_at").and_then(Value::as_str)
    }

    pub fn set_state(
        &mut self,
        state: RuleState,
    ) {
        self.extra.insert(
            "state".to_string(),
            Value::String(state.as_str().to_string()),
        );
    }

    pub fn set_body(
        &mut self,
        body: &str,
    ) {
        self.extra
            .insert("body".to_string(), Value::String(body.to_string()));
    }

    pub fn set_order_key(
        &mut self,
        order_key: i64,
    ) {
        self.extra
            .insert("order_key".to_string(), Value::Number(order_key.into()));
    }

    pub fn set_repo_scopes<I, S>(
        &mut self,
        scopes: I,
    ) where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.extra.insert(
            "repo_scopes".to_string(),
            Value::Array(scopes_to_json(scopes)),
        );
    }

    pub fn set_path_scopes<I, S>(
        &mut self,
        scopes: I,
    ) where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.extra.insert(
            "path_scopes".to_string(),
            Value::Array(scopes_to_json(scopes)),
        );
    }

    pub fn set_sentence_anchors<I, S>(
        &mut self,
        anchors: I,
    ) where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.extra.insert(
            "sentence_anchors".to_string(),
            Value::Array(scopes_to_json(anchors)),
        );
    }

    pub fn set_source_location(
        &mut self,
        source_repo: &str,
        source_path: &str,
        source_start_line: i64,
        source_end_line: i64,
    ) {
        self.extra.insert(
            "source_repo".to_string(),
            Value::String(source_repo.to_string()),
        );
        self.extra.insert(
            "source_path".to_string(),
            Value::String(source_path.to_string()),
        );
        self.extra.insert(
            "source_start_line".to_string(),
            Value::Number(Number::from(source_start_line)),
        );
        self.extra.insert(
            "source_end_line".to_string(),
            Value::Number(Number::from(source_end_line)),
        );
    }

    pub fn set_feedback_summary(
        &mut self,
        helpful_count: i64,
        mixed_count: i64,
        not_helpful_count: i64,
        note_count: i64,
        unresolved_count: i64,
        last_at: Option<&str>,
    ) {
        self.extra.insert(
            "feedback_helpful_count".to_string(),
            Value::Number(Number::from(helpful_count)),
        );
        self.extra.insert(
            "feedback_mixed_count".to_string(),
            Value::Number(Number::from(mixed_count)),
        );
        self.extra.insert(
            "feedback_not_helpful_count".to_string(),
            Value::Number(Number::from(not_helpful_count)),
        );
        self.extra.insert(
            "feedback_note_count".to_string(),
            Value::Number(Number::from(note_count)),
        );
        self.extra.insert(
            "feedback_unresolved_count".to_string(),
            Value::Number(Number::from(unresolved_count)),
        );
        match last_at {
            Some(timestamp) => {
                self.extra.insert(
                    "feedback_last_at".to_string(),
                    Value::String(timestamp.to_string()),
                );
            },
            None => {
                self.extra.remove("feedback_last_at");
            },
        }
    }
}

fn scopes_to_json<I, S>(values: I) -> Vec<Value>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    values
        .into_iter()
        .map(|value| Value::String(value.as_ref().to_string()))
        .collect()
}

fn string_array(
    extra: &BTreeMap<String, Value>,
    key: &str,
) -> Vec<String> {
    extra
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
