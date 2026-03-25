use std::collections::BTreeMap;
use std::process::Command;

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::CliRunError;

pub(crate) fn parse_uuid_field(cmd: &Value, field: &str) -> Result<Uuid, CliRunError> {
    cmd.get(field)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            CliRunError::InvalidExecPayload(format!("missing or invalid '{field}' field"))
        })
}

pub(crate) fn parse_fields(
    raw_fields: &[String],
) -> Result<BTreeMap<String, String>, CliRunError> {
    let mut fields = BTreeMap::new();
    for raw in raw_fields {
        let Some((k, v)) = raw.split_once('=') else {
            return Err(CliRunError::InvalidFieldPatch(raw.clone()));
        };
        fields.insert(k.trim().to_string(), v.trim().to_string());
    }
    Ok(fields)
}

pub(crate) fn parse_fields_to_json(
    raw_fields: &[String],
) -> Result<BTreeMap<String, Value>, CliRunError> {
    parse_fields(raw_fields).map(|m| {
        m.into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect()
    })
}

pub(crate) fn current_git_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let commit = value.trim();
    if commit.is_empty() {
        None
    } else {
        Some(commit.to_string())
    }
}

pub(crate) fn normalize_repro_timestamp(
    timestamp: Option<&str>,
) -> Result<String, CliRunError> {
    match timestamp {
        Some(raw) => chrono::DateTime::parse_from_rfc3339(raw)
            .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
            .map_err(|e| {
                CliRunError::BadRequest(format!(
                    "invalid --timestamp (expected RFC3339): {e}"
                ))
            }),
        None => Ok(Utc::now().to_rfc3339()),
    }
}

pub(crate) fn default_repro_summary() -> Value {
    json!({
        "count": 0,
        "last_outcome": Value::Null,
        "last_at": Value::Null,
        "last_commit": Value::Null,
    })
}

pub(crate) fn repro_summary_from_fields(fields: &BTreeMap<String, Value>) -> Value {
    let count = fields
        .get("reproductions")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    let last_outcome = fields
        .get("last_reproduction_outcome")
        .cloned()
        .unwrap_or(Value::Null);
    let last_at = fields
        .get("last_reproduced_at")
        .cloned()
        .unwrap_or(Value::Null);
    let last_commit = fields
        .get("last_reproduced_commit")
        .cloned()
        .unwrap_or(Value::Null);

    json!({
        "count": count,
        "last_outcome": last_outcome,
        "last_at": last_at,
        "last_commit": last_commit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fields_supports_key_values() {
        let got = parse_fields(&["owner=alice".to_string(), "priority=high".to_string()])
            .expect("field parsing should succeed");
        assert_eq!(got.get("owner").map(String::as_str), Some("alice"));
        assert_eq!(got.get("priority").map(String::as_str), Some("high"));
    }

    #[test]
    fn parse_fields_rejects_invalid_format() {
        let err = parse_fields(&["broken".to_string()]).expect_err("must reject missing '='");
        assert!(matches!(err, CliRunError::InvalidFieldPatch(_)));
    }

    #[test]
    fn normalize_repro_timestamp_accepts_rfc3339() {
        let got = normalize_repro_timestamp(Some("2026-03-22T12:34:56Z"))
            .expect("timestamp should parse");
        assert!(got.starts_with("2026-03-22T12:34:56"));
    }

    #[test]
    fn normalize_repro_timestamp_rejects_invalid_input() {
        let err = normalize_repro_timestamp(Some("not-a-timestamp"))
            .expect_err("invalid timestamp should fail");
        assert!(matches!(err, CliRunError::BadRequest(_)));
    }
}
