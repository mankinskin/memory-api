use chrono::Utc;
use ticket_api::model::ticket::TicketManifest;
use serde_json::json;
use uuid::Uuid;

#[test]
fn manifest_roundtrip_preserves_extra_fields() {
    let mut manifest = TicketManifest::new(Uuid::new_v4(), Utc::now());
    manifest
        .extra
        .insert("title".to_string(), json!("Implement watcher"));
    manifest
        .extra
        .insert("priority".to_string(), json!(1));

    let encoded = toml::to_string(&manifest).expect("manifest encodes to toml");
    let decoded: TicketManifest = toml::from_str(&encoded).expect("manifest decodes from toml");

    assert_eq!(decoded.id, manifest.id);
    assert_eq!(decoded.created_at, manifest.created_at);
    assert_eq!(decoded.extra.get("title"), manifest.extra.get("title"));
    assert_eq!(decoded.extra.get("priority"), manifest.extra.get("priority"));
}

#[test]
fn invalid_uuid_or_timestamp_fails_to_parse() {
    let bad_toml = r#"
id = "not-a-uuid"
created_at = "not-a-date"
"#;

    let parsed = toml::from_str::<TicketManifest>(bad_toml);
    assert!(parsed.is_err());
}
