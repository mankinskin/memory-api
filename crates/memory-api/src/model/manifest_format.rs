use std::fmt::Write as _;

use serde_json::Value;

use crate::model::entity::EntityManifest;

/// Canonical ordering for top-level fields in entity manifest files.
///
/// Fields listed here appear at the top of every entity file, in this exact
/// order.  Any additional fields not covered by this list are written in
/// **alphabetical order** after the priority fields.
pub const CANONICAL_FIELD_ORDER: &[&str] =
    &["id", "created_at", "title", "state", "acceptance_criteria"];

/// Serialize an `EntityManifest` to a canonical TOML string.
///
/// Fields are written in the order defined by [`CANONICAL_FIELD_ORDER`].
/// Fields not in that list follow in alphabetical order (the natural iteration
/// order of the `BTreeMap`-backed `extra` store).
pub fn format_manifest_toml(manifest: &EntityManifest) -> String {
    let mut out = String::new();

    // Identity fields — always present, always first.
    writeln!(out, "id = \"{}\"", manifest.id).unwrap();
    writeln!(out, "created_at = \"{}\"", manifest.created_at.to_rfc3339())
        .unwrap();

    // Priority extra fields in canonical order.
    let priority_extras = &CANONICAL_FIELD_ORDER[2..];
    for &key in priority_extras {
        if let Some(value) = manifest.extra.get(key) {
            write_toml_kv(&mut out, key, value);
        }
    }

    // Remaining extra fields in alphabetical order.
    let priority_set: std::collections::HashSet<&str> =
        priority_extras.iter().copied().collect();
    for (key, value) in &manifest.extra {
        if !priority_set.contains(key.as_str()) {
            write_toml_kv(&mut out, key, value);
        }
    }

    out
}

// ── field-order detection ─────────────────────────────────────────────────────

/// Returns `true` when every field in `toml_text` is already in canonical order.
pub fn is_canonically_ordered(toml_text: &str) -> bool {
    let actual = extract_key_order(toml_text);
    actual == canonical_order_for_keys(&actual)
}

/// Given the set of keys present in a manifest, compute the ordering that
/// [`format_manifest_toml`] would produce.
pub fn canonical_order_for_keys(keys: &[String]) -> Vec<String> {
    let key_set: std::collections::HashSet<&str> =
        keys.iter().map(|s| s.as_str()).collect();
    let mut result: Vec<String> = Vec::with_capacity(keys.len());

    for &canonical in CANONICAL_FIELD_ORDER {
        if key_set.contains(canonical) {
            result.push(canonical.to_string());
        }
    }

    let priority_set: std::collections::HashSet<&str> =
        CANONICAL_FIELD_ORDER.iter().copied().collect();
    let mut remainder: Vec<&str> = keys
        .iter()
        .filter(|k| !priority_set.contains(k.as_str()))
        .map(|s| s.as_str())
        .collect();
    remainder.sort_unstable();
    result.extend(remainder.iter().map(|&s| s.to_string()));

    result
}

/// Extract the top-level key ordering from a flat TOML text.
fn extract_key_order(toml_text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut in_multiline = false;

    for line in toml_text.lines() {
        let trimmed = line.trim();

        if in_multiline {
            if trimmed.contains("\"\"\"") {
                in_multiline = false;
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(eq_pos) = trimmed.find(" = ") {
            let key = trimmed[..eq_pos].trim().to_string();
            let value_part = trimmed[eq_pos + 3..].trim();

            if value_part.starts_with("\"\"\"") {
                let after_open = &value_part[3..];
                if !after_open.contains("\"\"\"") {
                    in_multiline = true;
                }
            }

            keys.push(key);
        }
    }

    keys
}

// ── value serialization ───────────────────────────────────────────────────────

fn write_toml_kv(
    out: &mut String,
    key: &str,
    value: &Value,
) {
    match value {
        Value::String(s) => {
            writeln!(out, "{key} = \"{}\"", escape_toml_basic(s)).unwrap();
        },
        Value::Number(n) => {
            writeln!(out, "{key} = {n}").unwrap();
        },
        Value::Bool(b) => {
            writeln!(out, "{key} = {b}").unwrap();
        },
        Value::Array(arr) => {
            let items: Vec<String> =
                arr.iter().map(inline_toml_value).collect();
            writeln!(out, "{key} = [{}]", items.join(", ")).unwrap();
        },
        Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k} = {}", inline_toml_value(v)))
                .collect();
            writeln!(out, "{key} = {{ {} }}", pairs.join(", ")).unwrap();
        },
        Value::Null => {
            // Omitting the key (rather than writing `key = ""`) is what
            // lets an update patch actually delete a manifest field; a
            // written-out null previously corrupted the field into a
            // permanent empty string instead of removing it.
        },
    }
}

fn inline_toml_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("\"{}\"", escape_toml_basic(s)),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(arr) => {
            let items: Vec<String> =
                arr.iter().map(inline_toml_value).collect();
            format!("[{}]", items.join(", "))
        },
        Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k} = {}", inline_toml_value(v)))
                .collect();
            format!("{{ {} }}", pairs.join(", "))
        },
        Value::Null => "\"\"".to_string(),
    }
}

fn escape_toml_basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\x08' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\x0c' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests;
