use std::fmt::Write as _;

use serde_json::Value;

use crate::model::entity::EntityManifest;

/// Canonical ordering for top-level fields in entity manifest files.
///
/// Fields listed here appear at the top of every entity file, in this exact
/// order.  Any additional fields not covered by this list are written in
/// **alphabetical order** after the priority fields.
pub const CANONICAL_FIELD_ORDER: &[&str] = &[
    "id",
    "created_at",
    "title",
    "state",
    "acceptance_criteria",
];

/// Serialize an `EntityManifest` to a canonical TOML string.
///
/// Fields are written in the order defined by [`CANONICAL_FIELD_ORDER`].
/// Fields not in that list follow in alphabetical order (the natural iteration
/// order of the `BTreeMap`-backed `extra` store).
pub fn format_manifest_toml(manifest: &EntityManifest) -> String {
    let mut out = String::new();

    // Identity fields — always present, always first.
    writeln!(out, "id = \"{}\"", manifest.id).unwrap();
    writeln!(out, "created_at = \"{}\"", manifest.created_at.to_rfc3339()).unwrap();

    // Priority extra fields in canonical order.
    let priority_extras = &CANONICAL_FIELD_ORDER[2..];
    for &key in priority_extras {
        if let Some(value) = manifest.extra.get(key) {
            write_toml_kv(&mut out, key, value);
        }
    }

    // Remaining extra fields in alphabetical order.
    let priority_set: std::collections::HashSet<&str> = priority_extras.iter().copied().collect();
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
    let key_set: std::collections::HashSet<&str> = keys.iter().map(|s| s.as_str()).collect();
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

fn write_toml_kv(out: &mut String, key: &str, value: &Value) {
    match value {
        Value::String(s) => {
            writeln!(out, "{key} = \"{}\"", escape_toml_basic(s)).unwrap();
        }
        Value::Number(n) => {
            writeln!(out, "{key} = {n}").unwrap();
        }
        Value::Bool(b) => {
            writeln!(out, "{key} = {b}").unwrap();
        }
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(inline_toml_value).collect();
            writeln!(out, "{key} = [{}]", items.join(", ")).unwrap();
        }
        Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k} = {}", inline_toml_value(v)))
                .collect();
            writeln!(out, "{key} = {{ {} }}", pairs.join(", ")).unwrap();
        }
        Value::Null => {
            writeln!(out, "{key} = \"\"").unwrap();
        }
    }
}

fn inline_toml_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("\"{}\"", escape_toml_basic(s)),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(inline_toml_value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k} = {}", inline_toml_value(v)))
                .collect();
            format!("{{ {} }}", pairs.join(", "))
        }
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

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::*;
    use crate::model::entity::EntityManifest;

    fn make_manifest(extra: &[(&str, &str)]) -> EntityManifest {
        let mut m = EntityManifest::new(Uuid::new_v4(), Utc::now());
        for (k, v) in extra {
            m.extra.insert(k.to_string(), Value::String(v.to_string()));
        }
        m
    }

    fn roundtrip(m: &EntityManifest) -> EntityManifest {
        let toml = format_manifest_toml(m);
        toml::from_str(&toml).expect("formatted TOML should be valid")
    }

    #[test]
    fn no_fields_dropped_string_fields() {
        let m = make_manifest(&[
            ("acceptance_criteria", "It works"),
            ("assigned_to", "alice"),
            ("component", "backend"),
            ("priority", "high"),
            ("risk_level", "low"),
            ("sprint", "2026-Q2"),
            ("state", "ready"),
            ("title", "Big feature"),
            ("type", "tracker-improvement"),
        ]);
        let parsed = roundtrip(&m);
        for key in m.extra.keys() {
            assert!(parsed.extra.contains_key(key), "field '{key}' was dropped");
            assert_eq!(parsed.extra[key], m.extra[key], "field '{key}' value was altered");
        }
        assert_eq!(parsed.extra.len(), m.extra.len(), "field count changed");
    }

    #[test]
    fn no_fields_dropped_with_boolean_and_integer() {
        let mut m = EntityManifest::new(Uuid::new_v4(), Utc::now());
        m.extra.insert("title".into(), json!("ticket"));
        m.extra.insert("active".into(), json!(true));
        m.extra.insert("inactive".into(), json!(false));
        m.extra.insert("count".into(), json!(99));
        m.extra.insert("zero".into(), json!(0));
        m.extra.insert("negative".into(), json!(-7));

        let parsed = roundtrip(&m);
        assert_eq!(parsed.extra.len(), m.extra.len(), "field count changed");
        for key in m.extra.keys() {
            assert!(parsed.extra.contains_key(key), "field '{key}' was dropped");
            assert_eq!(parsed.extra[key], m.extra[key], "field '{key}' modified");
        }
    }

    #[test]
    fn no_fields_dropped_large_field_set() {
        let mut m = EntityManifest::new(Uuid::new_v4(), Utc::now());
        let fields: Vec<(&str, &str)> = vec![
            ("a_first", "alpha"),
            ("acceptance_criteria", "meets all AC"),
            ("assigned_to", "bob"),
            ("b_second", "beta"),
            ("category", "infra"),
            ("component", "scheduler"),
            ("created_by", "ci"),
            ("d_delta", "d"),
            ("e_epsilon", "e"),
            ("environment", "production"),
            ("f_field", "f"),
            ("g_gamma", "g"),
            ("h_hotel", "h"),
            ("impact", "high"),
            ("labels", "perf,latency"),
            ("milestone", "v3.0"),
            ("owner", "alice"),
            ("priority", "critical"),
            ("risk_level", "medium"),
            ("state", "in-review"),
            ("title", "Large entity"),
            ("type", "tracker-improvement"),
            ("ux_impact", "none"),
            ("validated_by", "qa"),
            ("version", "1.2.3"),
            ("w_whiskey", "w"),
            ("x_xray", "x"),
            ("y_yankee", "y"),
            ("z_zulu", "z"),
        ];
        for (k, v) in &fields {
            m.extra.insert(k.to_string(), Value::String(v.to_string()));
        }

        let parsed = roundtrip(&m);
        assert_eq!(parsed.extra.len(), m.extra.len(), "field count changed");
        for (key, _) in &fields {
            assert!(parsed.extra.contains_key(*key), "field '{key}' was dropped");
            assert_eq!(parsed.extra[*key], m.extra[*key], "field '{key}' value was modified");
        }
    }

    #[test]
    fn manifest_partialeq_holds_after_roundtrip() {
        let mut m = EntityManifest::new(Uuid::new_v4(), Utc::now());
        m.extra.insert("title".into(), json!("roundtrip test"));
        m.extra.insert("state".into(), json!("new"));
        m.extra.insert("acceptance_criteria".into(), json!("must pass"));
        m.extra.insert("priority".into(), json!("medium"));
        m.extra.insert("component".into(), json!("api"));
        m.extra.insert("type".into(), json!("tracker-improvement"));
        m.extra.insert("active".into(), json!(true));
        m.extra.insert("count".into(), json!(7));

        let parsed = roundtrip(&m);
        assert_eq!(parsed, m, "roundtripped manifest does not equal original");
    }

    #[test]
    fn created_at_is_preserved_exactly() {
        let fixed = chrono::DateTime::parse_from_rfc3339("2026-04-08T14:20:50.462259100+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let m = EntityManifest::new(Uuid::new_v4(), fixed);
        let parsed = roundtrip(&m);
        assert_eq!(parsed.created_at, m.created_at, "created_at was altered");
    }

    #[test]
    fn roundtrip_string_with_double_quotes() {
        let m = make_manifest(&[("title", r#"Hello "World""#)]);
        let parsed = roundtrip(&m);
        assert_eq!(parsed.extra["title"], m.extra["title"]);
    }

    #[test]
    fn roundtrip_string_with_backslash() {
        let m = make_manifest(&[("path_hint", r#"C:\Users\foo\bar"#)]);
        let parsed = roundtrip(&m);
        assert_eq!(parsed.extra["path_hint"], m.extra["path_hint"]);
    }

    #[test]
    fn roundtrip_string_with_embedded_newline() {
        let m = make_manifest(&[("acceptance_criteria", "line one\nline two\nline three")]);
        let parsed = roundtrip(&m);
        assert_eq!(parsed.extra["acceptance_criteria"], m.extra["acceptance_criteria"]);
    }

    #[test]
    fn roundtrip_string_with_embedded_tab() {
        let m = make_manifest(&[("note", "col1\tcol2\tcol3")]);
        let parsed = roundtrip(&m);
        assert_eq!(parsed.extra["note"], m.extra["note"]);
    }

    #[test]
    fn roundtrip_string_with_unicode() {
        let m = make_manifest(&[("title", "Ünïcödé: 日本語 🎉")]);
        let parsed = roundtrip(&m);
        assert_eq!(parsed.extra["title"], m.extra["title"]);
    }

    #[test]
    fn roundtrip_string_with_mixed_special_chars() {
        let value = "path: \"C:\\tmp\"\nnext line";
        let m = make_manifest(&[("note", value)]);
        let parsed = roundtrip(&m);
        assert_eq!(parsed.extra["note"], Value::String(value.to_string()));
    }

    #[test]
    fn formatting_is_idempotent() {
        let mut m = EntityManifest::new(Uuid::new_v4(), Utc::now());
        m.extra.insert("title".into(), json!("idempotent"));
        m.extra.insert("state".into(), json!("new"));
        m.extra.insert("acceptance_criteria".into(), json!("pass\nall tests"));
        m.extra.insert("priority".into(), json!("high"));
        m.extra.insert("component".into(), json!("core"));
        m.extra.insert("active".into(), json!(false));
        m.extra.insert("count".into(), json!(3));

        let first_format = format_manifest_toml(&m);
        let reparsed: EntityManifest = toml::from_str(&first_format).unwrap();
        let second_format = format_manifest_toml(&reparsed);

        assert_eq!(first_format, second_format, "formatting is not idempotent");
    }

    #[test]
    fn formatting_is_idempotent_after_is_canonically_ordered_check() {
        let m = make_manifest(&[
            ("zzz_last", "z"),
            ("state", "ready"),
            ("acceptance_criteria", "done"),
            ("title", "idempotency"),
            ("aaa_first", "a"),
        ]);
        let formatted = format_manifest_toml(&m);
        assert!(is_canonically_ordered(&formatted), "formatted output not canonically ordered");
        let reparsed: EntityManifest = toml::from_str(&formatted).unwrap();
        assert_eq!(format_manifest_toml(&reparsed), formatted);
    }

    #[test]
    fn canonical_order_puts_priority_fields_first() {
        let m = make_manifest(&[
            ("component", "api"),
            ("state", "new"),
            ("title", "My entity"),
            ("acceptance_criteria", "It works"),
            ("priority", "high"),
        ]);
        let toml = format_manifest_toml(&m);
        let keys = extract_key_order(&toml);
        assert_eq!(keys[0], "id");
        assert_eq!(keys[1], "created_at");
        assert_eq!(keys[2], "title");
        assert_eq!(keys[3], "state");
        assert_eq!(keys[4], "acceptance_criteria");
        assert_eq!(keys[5], "component");
        assert_eq!(keys[6], "priority");
    }

    #[test]
    fn missing_priority_fields_are_skipped_not_gap_filled() {
        let m = make_manifest(&[("title", "x"), ("zzz", "last")]);
        let toml = format_manifest_toml(&m);
        let keys = extract_key_order(&toml);
        assert_eq!(keys, vec!["id", "created_at", "title", "zzz"]);
    }

    #[test]
    fn roundtrip_boolean_and_number() {
        let mut m = EntityManifest::new(Uuid::new_v4(), Utc::now());
        m.extra.insert("active".to_string(), Value::Bool(true));
        m.extra.insert("count".to_string(), Value::Number(42.into()));
        let toml = format_manifest_toml(&m);
        let parsed: EntityManifest = toml::from_str(&toml).unwrap();
        assert_eq!(parsed.extra["active"], Value::Bool(true));
        assert_eq!(parsed.extra["count"], Value::Number(42.into()));
    }

    #[test]
    fn is_canonically_ordered_detects_wrong_order() {
        let toml = "id = \"1\"\ncreated_at = \"t\"\nstate = \"new\"\ntitle = \"x\"\n";
        assert!(!is_canonically_ordered(toml));
    }

    #[test]
    fn is_canonically_ordered_accepts_correct_order() {
        let toml = "id = \"1\"\ncreated_at = \"t\"\ntitle = \"x\"\nstate = \"new\"\n";
        assert!(is_canonically_ordered(toml));
    }

    #[test]
    fn is_canonically_ordered_accepts_format_manifest_output() {
        let m = make_manifest(&[
            ("component", "api"),
            ("state", "new"),
            ("title", "My entity"),
            ("acceptance_criteria", "It works"),
            ("priority", "high"),
        ]);
        let toml = format_manifest_toml(&m);
        assert!(is_canonically_ordered(&toml));
    }

    #[test]
    fn is_canonically_ordered_handles_multiline_values() {
        let toml = concat!(
            "id = \"1\"\n",
            "created_at = \"t\"\n",
            "title = \"x\"\n",
            "acceptance_criteria = \"\"\"\n",
            "state = \"this is content, not a key\"\n",
            "\"\"\"\n",
            "state = \"new\"\n",
        );
        assert!(!is_canonically_ordered(toml));
    }

    #[test]
    fn canonical_order_for_keys_sorts_remainder_alphabetically() {
        let keys: Vec<String> = vec![
            "zzz".into(),
            "id".into(),
            "created_at".into(),
            "aaa".into(),
            "title".into(),
        ];
        let ordered = canonical_order_for_keys(&keys);
        assert_eq!(ordered, vec!["id", "created_at", "title", "aaa", "zzz"]);
    }
}
