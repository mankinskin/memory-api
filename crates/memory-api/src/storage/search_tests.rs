use super::*;

use super::*;

#[test]
fn rebuilds_search_index_for_tantivy_thread_panic_messages() {
    let error = StorageError::SearchIndex(
            "storage error: search index error: An error occurred in a thread: 'Any { .. }'"
                .to_string(),
        );

    assert!(TantivySearchIndex::should_rebuild_search_index(&error));
}

#[test]
fn rebuilds_search_index_for_index_out_of_bounds_panic() {
    let error = StorageError::SearchIndex(
        "index out of bounds: the len is 5 but the index is 5".to_string(),
    );

    assert!(TantivySearchIndex::should_rebuild_search_index(&error));
}

#[test]
fn does_not_rebuild_search_index_for_retryable_permission_errors() {
    let error = StorageError::SearchIndex("permission denied".to_string());

    assert!(!TantivySearchIndex::should_rebuild_search_index(&error));
}

#[test]
fn read_failure_rebuilds_for_any_non_transient_search_error() {
    // Segment-content damage surfaces with many different messages, none of
    // which the substring classifier covers — all must trigger a rebuild.
    for message in [
        "Failed to open file for read: FileDoesNotExist(\"seg.term\")",
        "File corrupted. The file is smaller than 4 bytes (len=2).",
        "UnexpectedEof while reading segment",
        "schema mismatch in segment",
    ] {
        let error = StorageError::SearchIndex(message.to_string());
        assert!(
            TantivySearchIndex::is_rebuildable_read_failure(&error),
            "expected rebuild for: {message}"
        );
    }
}

#[test]
fn read_failure_does_not_rebuild_for_transient_permission_errors() {
    for message in [
        "PermissionDenied (os error 5)",
        "Access is denied. (os error 5)",
    ] {
        let error = StorageError::SearchIndex(message.to_string());
        assert!(
            !TantivySearchIndex::is_rebuildable_read_failure(&error),
            "expected no rebuild for transient: {message}"
        );
    }

    // Non-search errors are never rebuildable.
    assert!(!TantivySearchIndex::is_rebuildable_read_failure(
        &StorageError::DependencyCycle
    ));
}

#[test]
fn open_or_create_heals_stale_fast_field_schema_on_construction() {
    let dir = tempfile::tempdir().unwrap();
    let index_dir = dir.path().join("search_index");
    std::fs::create_dir_all(&index_dir).unwrap();

    // Build an OLDER 5-field schema (before `created_at`/`effort` fast
    // fields existed) so the on-disk layout no longer matches.
    let mut builder = Schema::builder();
    builder.add_text_field("id", STRING | STORED);
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("body", TEXT | STORED);
    builder.add_text_field("state", STRING | STORED | FAST);
    builder.add_text_field("ticket_type", STRING | STORED | FAST);
    Index::create_in_dir(&index_dir, builder.build()).unwrap();

    // Construction proactively validates and heals the stale schema, so the
    // index is already current before any operation runs.
    let search = TantivySearchIndex::open_or_create(&index_dir).unwrap();
    assert_eq!(search.check_invariants().unwrap(), None);

    // A write that references the newer fast fields must succeed (the
    // fast-field writer must not index past a stale field count).
    let id = Uuid::new_v4();
    search
        .upsert(
            &id,
            Some("title"),
            Some("searchable body"),
            Some("ready"),
            Some("rule-entry"),
            Some("2026-06-15T00:00:00Z"),
            Some("3"),
        )
        .unwrap();

    let expr = crate::model::query::parse_query("searchable").unwrap();
    assert_eq!(search.search(&expr, 10).unwrap().len(), 1);
}

#[test]
fn check_invariants_detects_then_heals_stale_schema() {
    let dir = tempfile::tempdir().unwrap();
    let index_dir = dir.path().join("search_index");

    // Start from a current, healthy index.
    let search = TantivySearchIndex::open_or_create(&index_dir).unwrap();
    assert_eq!(search.check_invariants().unwrap(), None);

    // Replace the on-disk index with an older 5-field schema behind the
    // existing handle (simulating a schema change across versions).
    std::fs::remove_dir_all(&index_dir).unwrap();
    std::fs::create_dir_all(&index_dir).unwrap();
    let mut builder = Schema::builder();
    builder.add_text_field("id", STRING | STORED);
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("body", TEXT | STORED);
    builder.add_text_field("state", STRING | STORED | FAST);
    builder.add_text_field("ticket_type", STRING | STORED | FAST);
    Index::create_in_dir(&index_dir, builder.build()).unwrap();

    // The read-only check detects the stale schema...
    assert_eq!(
        search.check_invariants().unwrap(),
        Some(IndexInvariant::SchemaCurrent)
    );

    // ...and the proactive gate heals it before any operation proceeds.
    search.ensure_schema_current().unwrap();
    assert_eq!(search.check_invariants().unwrap(), None);

    let id = Uuid::new_v4();
    search
        .upsert(&id, Some("t"), Some("healed token"), None, None, None, None)
        .unwrap();
    let expr = crate::model::query::parse_query("token").unwrap();
    assert_eq!(search.search(&expr, 10).unwrap().len(), 1);
}

#[test]
fn check_invariants_detects_then_heals_corrupt_index() {
    let dir = tempfile::tempdir().unwrap();
    let index_dir = dir.path().join("search_index");

    let search = TantivySearchIndex::open_or_create(&index_dir).unwrap();
    let id = Uuid::new_v4();
    search
        .upsert(
            &id,
            Some("before"),
            Some("before body"),
            None,
            None,
            None,
            None,
        )
        .unwrap();

    // Corrupt the index metadata so it can no longer be opened.
    std::fs::write(index_dir.join("meta.json"), b"not valid json").unwrap();
    assert_eq!(
        search.check_invariants().unwrap(),
        Some(IndexInvariant::Openable)
    );

    // The next operation proactively rebuilds the index from the current
    // schema instead of erroring out.
    let id2 = Uuid::new_v4();
    search
        .upsert(
            &id2,
            Some("after"),
            Some("after token"),
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(search.check_invariants().unwrap(), None);

    let expr = crate::model::query::parse_query("token").unwrap();
    assert_eq!(search.search(&expr, 10).unwrap().len(), 1);
}

#[test]
fn ensure_schema_current_keeps_current_schema_intact() {
    let dir = tempfile::tempdir().unwrap();
    let index_dir = dir.path().join("search_index");

    let search = TantivySearchIndex::open_or_create(&index_dir).unwrap();
    let id = Uuid::new_v4();
    search
        .upsert(&id, Some("keep"), Some("keep body"), None, None, None, None)
        .unwrap();

    assert_eq!(search.check_invariants().unwrap(), None);
    // No-op when the schema already matches: the existing document survives.
    search.ensure_schema_current().unwrap();

    let expr = crate::model::query::parse_query("keep").unwrap();
    assert_eq!(search.search(&expr, 10).unwrap().len(), 1);
}
