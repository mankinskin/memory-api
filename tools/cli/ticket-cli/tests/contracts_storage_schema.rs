use ticket_api::storage::schema::{
    REQUIRED_TABLES,
    SCHEMA_VERSION,
    TABLE_EDGES,
    TABLE_LEASES,
    TABLE_META,
    TABLE_SCAN_ROOTS,
    TABLE_TICKETS,
    ensure_supported_schema_version,
};

#[test]
fn required_tables_are_stable_and_version_gated() {
    assert_eq!(REQUIRED_TABLES.len(), 5);
    assert_eq!(REQUIRED_TABLES[0], TABLE_TICKETS);
    assert_eq!(REQUIRED_TABLES[1], TABLE_EDGES);
    assert_eq!(REQUIRED_TABLES[2], TABLE_SCAN_ROOTS);
    assert_eq!(REQUIRED_TABLES[3], TABLE_LEASES);
    assert_eq!(REQUIRED_TABLES[4], TABLE_META);

    ensure_supported_schema_version(SCHEMA_VERSION).expect("current version must be supported");
}

#[test]
fn mismatched_schema_version_returns_actionable_error() {
    let err = ensure_supported_schema_version("999")
        .expect_err("mismatch should produce an actionable migration error");

    let message = err.to_string();
    assert!(message.contains("schema version mismatch"));
    assert!(message.contains("ticket scan --reindex"));
    assert!(message.contains("schema upgrade"));
}
