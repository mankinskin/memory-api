use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::model::entity::EntityManifest;

fn make_manifest(extra: &[(&str, &str)]) -> EntityManifest {
    let mut manifest = EntityManifest::new(Uuid::new_v4(), Utc::now());
    for (key, value) in extra {
        manifest
            .extra
            .insert(key.to_string(), Value::String(value.to_string()));
    }
    manifest
}

fn roundtrip(manifest: &EntityManifest) -> EntityManifest {
    let toml = format_manifest_toml(manifest);
    toml::from_str(&toml).expect("formatted TOML should be valid")
}

#[test]
fn no_fields_dropped_string_fields() {
    let manifest = make_manifest(&[
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
    let parsed = roundtrip(&manifest);
    for key in manifest.extra.keys() {
        assert!(parsed.extra.contains_key(key), "field '{key}' was dropped");
        assert_eq!(parsed.extra[key], manifest.extra[key], "field '{key}' value was altered");
    }
    assert_eq!(parsed.extra.len(), manifest.extra.len(), "field count changed");
}

#[test]
fn no_fields_dropped_with_boolean_and_integer() {
    let mut manifest = EntityManifest::new(Uuid::new_v4(), Utc::now());
    manifest.extra.insert("title".into(), json!("ticket"));
    manifest.extra.insert("active".into(), json!(true));
    manifest.extra.insert("inactive".into(), json!(false));
    manifest.extra.insert("count".into(), json!(99));
    manifest.extra.insert("zero".into(), json!(0));
    manifest.extra.insert("negative".into(), json!(-7));

    let parsed = roundtrip(&manifest);
    assert_eq!(parsed.extra.len(), manifest.extra.len(), "field count changed");
    for key in manifest.extra.keys() {
        assert!(parsed.extra.contains_key(key), "field '{key}' was dropped");
        assert_eq!(parsed.extra[key], manifest.extra[key], "field '{key}' modified");
    }
}

#[test]
fn no_fields_dropped_large_field_set() {
    let mut manifest = EntityManifest::new(Uuid::new_v4(), Utc::now());
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
    for (key, value) in &fields {
        manifest
            .extra
            .insert(key.to_string(), Value::String(value.to_string()));
    }

    let parsed = roundtrip(&manifest);
    assert_eq!(parsed.extra.len(), manifest.extra.len(), "field count changed");
    for (key, _) in &fields {
        assert!(parsed.extra.contains_key(*key), "field '{key}' was dropped");
        assert_eq!(parsed.extra[*key], manifest.extra[*key], "field '{key}' value was modified");
    }
}

#[test]
fn manifest_partialeq_holds_after_roundtrip() {
    let mut manifest = EntityManifest::new(Uuid::new_v4(), Utc::now());
    manifest.extra.insert("title".into(), json!("roundtrip test"));
    manifest.extra.insert("state".into(), json!("new"));
    manifest
        .extra
        .insert("acceptance_criteria".into(), json!("must pass"));
    manifest.extra.insert("priority".into(), json!("medium"));
    manifest.extra.insert("component".into(), json!("api"));
    manifest.extra.insert("type".into(), json!("tracker-improvement"));
    manifest.extra.insert("active".into(), json!(true));
    manifest.extra.insert("count".into(), json!(7));

    let parsed = roundtrip(&manifest);
    assert_eq!(parsed, manifest, "roundtripped manifest does not equal original");
}

#[test]
fn created_at_is_preserved_exactly() {
    let fixed = chrono::DateTime::parse_from_rfc3339("2026-04-08T14:20:50.462259100+00:00")
        .unwrap()
        .with_timezone(&Utc);
    let manifest = EntityManifest::new(Uuid::new_v4(), fixed);
    let parsed = roundtrip(&manifest);
    assert_eq!(parsed.created_at, manifest.created_at, "created_at was altered");
}

#[test]
fn roundtrip_string_with_double_quotes() {
    let manifest = make_manifest(&[("title", r#"Hello \"World\""#)]);
    let parsed = roundtrip(&manifest);
    assert_eq!(parsed.extra["title"], manifest.extra["title"]);
}

#[test]
fn roundtrip_string_with_backslash() {
    let manifest = make_manifest(&[("path_hint", r#"C:\Users\foo\bar"#)]);
    let parsed = roundtrip(&manifest);
    assert_eq!(parsed.extra["path_hint"], manifest.extra["path_hint"]);
}

#[test]
fn roundtrip_string_with_embedded_newline() {
    let manifest = make_manifest(&[("acceptance_criteria", "line one\nline two\nline three")]);
    let parsed = roundtrip(&manifest);
    assert_eq!(parsed.extra["acceptance_criteria"], manifest.extra["acceptance_criteria"]);
}

#[test]
fn roundtrip_string_with_embedded_tab() {
    let manifest = make_manifest(&[("note", "col1\tcol2\tcol3")]);
    let parsed = roundtrip(&manifest);
    assert_eq!(parsed.extra["note"], manifest.extra["note"]);
}

#[test]
fn roundtrip_string_with_unicode() {
    let manifest = make_manifest(&[("title", "Ünïcödé: 日本語 🎉")]);
    let parsed = roundtrip(&manifest);
    assert_eq!(parsed.extra["title"], manifest.extra["title"]);
}

#[test]
fn roundtrip_string_with_mixed_special_chars() {
    let value = "path: \"C:\\tmp\"\nnext line";
    let manifest = make_manifest(&[("note", value)]);
    let parsed = roundtrip(&manifest);
    assert_eq!(parsed.extra["note"], Value::String(value.to_string()));
}

#[test]
fn formatting_is_idempotent() {
    let mut manifest = EntityManifest::new(Uuid::new_v4(), Utc::now());
    manifest.extra.insert("title".into(), json!("idempotent"));
    manifest.extra.insert("state".into(), json!("new"));
    manifest
        .extra
        .insert("acceptance_criteria".into(), json!("pass\nall tests"));
    manifest.extra.insert("priority".into(), json!("high"));
    manifest.extra.insert("component".into(), json!("core"));
    manifest.extra.insert("active".into(), json!(false));
    manifest.extra.insert("count".into(), json!(3));

    let first_format = format_manifest_toml(&manifest);
    let reparsed: EntityManifest = toml::from_str(&first_format).unwrap();
    let second_format = format_manifest_toml(&reparsed);

    assert_eq!(first_format, second_format, "formatting is not idempotent");
}

#[test]
fn formatting_is_idempotent_after_is_canonically_ordered_check() {
    let manifest = make_manifest(&[
        ("zzz_last", "z"),
        ("state", "ready"),
        ("acceptance_criteria", "done"),
        ("title", "idempotency"),
        ("aaa_first", "a"),
    ]);
    let formatted = format_manifest_toml(&manifest);
    assert!(is_canonically_ordered(&formatted), "formatted output not canonically ordered");
    let reparsed: EntityManifest = toml::from_str(&formatted).unwrap();
    assert_eq!(format_manifest_toml(&reparsed), formatted);
}

#[test]
fn canonical_order_puts_priority_fields_first() {
    let manifest = make_manifest(&[
        ("component", "api"),
        ("state", "new"),
        ("title", "My entity"),
        ("acceptance_criteria", "It works"),
        ("priority", "high"),
    ]);
    let toml = format_manifest_toml(&manifest);
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
    let manifest = make_manifest(&[("title", "x"), ("zzz", "last")]);
    let toml = format_manifest_toml(&manifest);
    let keys = extract_key_order(&toml);
    assert_eq!(keys, vec!["id", "created_at", "title", "zzz"]);
}

#[test]
fn roundtrip_boolean_and_number() {
    let mut manifest = EntityManifest::new(Uuid::new_v4(), Utc::now());
    manifest.extra.insert("active".to_string(), Value::Bool(true));
    manifest.extra.insert("count".to_string(), Value::Number(42.into()));
    let toml = format_manifest_toml(&manifest);
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
    let manifest = make_manifest(&[
        ("component", "api"),
        ("state", "new"),
        ("title", "My entity"),
        ("acceptance_criteria", "It works"),
        ("priority", "high"),
    ]);
    let toml = format_manifest_toml(&manifest);
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