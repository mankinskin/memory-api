use std::collections::BTreeMap;

use serde_json::Value;

use super::{
    HistoryRevision,
    TicketFs,
};
use crate::model::ticket::{
    SpecRef,
    TicketManifest,
    TicketManifestExt,
    TicketPart,
    TicketRefEntry,
};

#[test]
fn history_revision_backward_compat_no_author() {
    let json = r#"{"rev":1,"ts":"2025-01-01T00:00:00Z","fields":{"state":"open","title":"Old entry"}}"#;
    let rev: HistoryRevision = serde_json::from_str(json)
        .expect("should deserialize legacy revision without author field");
    assert_eq!(rev.rev, 1);
    assert_eq!(rev.author, None, "author should be None for legacy entries");
}

#[test]
fn history_revision_with_author() {
    let json =
        r#"{"rev":2,"ts":"2025-01-02T00:00:00Z","fields":{},"author":"alice"}"#;
    let rev: HistoryRevision = serde_json::from_str(json)
        .expect("should deserialize revision with author");
    assert_eq!(rev.author, Some("alice".to_string()));
}

#[test]
fn history_revision_none_author_is_skipped_in_serialization() {
    let rev = HistoryRevision {
        rev: 1,
        ts: "2025-01-01T00:00:00Z".to_string(),
        fields: BTreeMap::new(),
        author: None,
    };
    let json = serde_json::to_string(&rev).expect("serialize");
    let value: Value = serde_json::from_str(&json).unwrap();
    assert!(
        value.get("author").is_none(),
        "author key should be absent when None"
    );
}

#[test]
fn read_history_skips_malformed_line_instead_of_erroring() {
    let dir = tempfile::tempdir().unwrap();
    let ticket_path = dir.path().join("ticket");
    std::fs::create_dir_all(&ticket_path).unwrap();
    std::fs::write(
        ticket_path.join("history.ndjson"),
        "[]\n{\"rev\":1,\"ts\":\"2025-01-01T00:00:00Z\",\"fields\":{}}\n",
    )
    .unwrap();

    let revisions = TicketFs::read_history(&ticket_path)
        .expect("a malformed line must not fail the read");
    assert_eq!(revisions.len(), 1, "the one valid revision should survive");
    assert_eq!(revisions[0].rev, 1);
}

#[test]
fn append_history_still_works_after_a_malformed_line() {
    let dir = tempfile::tempdir().unwrap();
    let ticket_path = dir.path().join("ticket");
    std::fs::create_dir_all(&ticket_path).unwrap();
    // Simulates the real corruption found in production: a stray `[]` line.
    std::fs::write(ticket_path.join("history.ndjson"), "[]\n").unwrap();

    let rev = TicketFs::append_history(&ticket_path, BTreeMap::new(), None)
        .expect(
            "append must still succeed even though history.ndjson has a \
             malformed line",
        );
    assert_eq!(rev, 1, "malformed line is skipped, not counted as a revision");

    let revisions = TicketFs::read_history(&ticket_path).unwrap();
    assert_eq!(revisions.len(), 1, "the newly appended revision is readable");
}

#[test]
fn update_with_null_patch_value_removes_the_key() {
    let (_dir, ticket_path) = make_ticket_dir();
    let mut patch = BTreeMap::new();
    patch.insert(
        "handoff_package".to_string(),
        Value::String("stale data".to_string()),
    );
    TicketFs::update(&ticket_path, &patch, None).unwrap();

    let mut delete_patch = BTreeMap::new();
    delete_patch.insert("handoff_package".to_string(), Value::Null);
    let manifest =
        TicketFs::update(&ticket_path, &delete_patch, None).unwrap();

    assert!(
        !manifest.extra.contains_key("handoff_package"),
        "null patch value must delete the key, not corrupt it to an empty string"
    );
    let reloaded = TicketFs::read(&ticket_path).unwrap();
    assert!(
        !reloaded.extra.contains_key("handoff_package"),
        "deletion must persist across a reload from disk"
    );
}

#[test]
fn update_with_explicit_empty_string_is_not_treated_as_deletion() {
    let (_dir, ticket_path) = make_ticket_dir();
    let mut patch = BTreeMap::new();
    patch.insert("notes".to_string(), Value::String(String::new()));
    let manifest = TicketFs::update(&ticket_path, &patch, None).unwrap();

    assert_eq!(
        manifest.extra.get("notes"),
        Some(&Value::String(String::new())),
        "an explicit empty string must be preserved, not deleted"
    );
}

// ── ticket parts: model + storage read path (spec 24b3d22b, AC1-4, 7-9) ──────

fn make_ticket_dir() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let id = uuid::Uuid::new_v4();
    let ticket_path = dir.path().join(id.to_string());
    std::fs::create_dir_all(&ticket_path).unwrap();
    let manifest = TicketManifest::new(id, chrono::Utc::now());
    std::fs::write(
        ticket_path.join("ticket.toml"),
        crate::model::manifest_format::format_manifest_toml(&manifest),
    )
    .unwrap();
    (dir, ticket_path)
}

fn write_part_file(
    ticket_path: &std::path::Path,
    part: &TicketPart,
    content: &str,
) {
    std::fs::create_dir_all(ticket_path.join("parts")).unwrap();
    std::fs::write(ticket_path.join(&part.path), content).unwrap();
}

#[test]
fn legacy_ticket_with_no_parts_table_loads_implicit_objective() {
    let (_dir, ticket_path) = make_ticket_dir();
    std::fs::write(
        ticket_path.join("description.md"),
        "Original objective text",
    )
    .unwrap();
    let manifest = TicketFs::read(&ticket_path).unwrap();
    assert!(
        manifest.parts().is_empty(),
        "legacy manifest should have no [[parts]] entries"
    );

    let report = TicketFs::load_parts(&ticket_path, &manifest).unwrap();

    assert_eq!(report.parts.len(), 1);
    let objective = &report.parts[0];
    assert_eq!(objective.kind, "objective");
    assert!(objective.implicit);
    assert_eq!(objective.content, "Original objective text");
    assert!(report.orphans.is_empty());

    // The synthetic id must be stable across independent reads.
    let report_again = TicketFs::load_parts(&ticket_path, &manifest).unwrap();
    assert_eq!(report_again.parts[0].id, objective.id);
}

#[test]
fn parts_are_addressed_by_id_not_kind_or_index() {
    let (_dir, ticket_path) = make_ticket_dir();
    let mut manifest = TicketFs::read(&ticket_path).unwrap();

    let review_a = TicketPart::new("review", "parts/review-a.md");
    let review_b = TicketPart::new("review", "parts/review-b.md");
    write_part_file(&ticket_path, &review_a, "first pass");
    write_part_file(&ticket_path, &review_b, "second pass");
    manifest.set_parts(vec![review_a.clone(), review_b.clone()]);
    std::fs::write(
        ticket_path.join("ticket.toml"),
        crate::model::manifest_format::format_manifest_toml(&manifest),
    )
    .unwrap();

    let manifest = TicketFs::read(&ticket_path).unwrap();
    let report = TicketFs::load_parts(&ticket_path, &manifest).unwrap();

    // Two parts of the same kind: distinct ids keep them individually
    // addressable and manifest order is preserved.
    assert_eq!(report.parts.len(), 2);
    assert_eq!(report.parts[0].id, review_a.id);
    assert_eq!(report.parts[1].id, review_b.id);
    assert_eq!(report.find(review_a.id).unwrap().content, "first pass");
    assert_eq!(report.find(review_b.id).unwrap().content, "second pass");
    assert_eq!(report.of_kind("review").count(), 2);
    assert!(report.find(uuid::Uuid::new_v4()).is_none());
}

#[test]
fn orphan_part_file_is_reported_not_silently_adopted() {
    let (_dir, ticket_path) = make_ticket_dir();
    let mut manifest = TicketFs::read(&ticket_path).unwrap();

    let objective = TicketPart::new("objective", "parts/objective.md");
    write_part_file(&ticket_path, &objective, "the objective");
    manifest.set_parts(vec![objective.clone()]);
    std::fs::write(
        ticket_path.join("ticket.toml"),
        crate::model::manifest_format::format_manifest_toml(&manifest),
    )
    .unwrap();

    // A file dropped into parts/ with no manifest entry.
    std::fs::write(
        ticket_path.join("parts").join("stray.md"),
        "not indexed anywhere",
    )
    .unwrap();

    let manifest = TicketFs::read(&ticket_path).unwrap();
    let report = TicketFs::load_parts(&ticket_path, &manifest).unwrap();

    assert_eq!(report.parts.len(), 1, "only the indexed part loads");
    assert_eq!(report.orphans.len(), 1);
    assert_eq!(
        report.orphans[0].file_name().unwrap().to_str().unwrap(),
        "stray.md"
    );
}

#[test]
fn unknown_kind_part_round_trips_as_opaque_attachment() {
    let (_dir, ticket_path) = make_ticket_dir();
    let mut manifest = TicketFs::read(&ticket_path).unwrap();

    let attachment =
        TicketPart::new("handoff_package", "parts/handoff.md");
    write_part_file(&ticket_path, &attachment, "opaque payload");
    manifest.set_parts(vec![attachment.clone()]);
    std::fs::write(
        ticket_path.join("ticket.toml"),
        crate::model::manifest_format::format_manifest_toml(&manifest),
    )
    .unwrap();

    let manifest = TicketFs::read(&ticket_path).unwrap();
    let report = TicketFs::load_parts(&ticket_path, &manifest).unwrap();

    assert_eq!(report.parts.len(), 1);
    let loaded = &report.parts[0];
    assert_eq!(loaded.kind, "handoff_package");
    assert_eq!(loaded.content, "opaque payload");
    assert!(
        !crate::model::parts::is_core_part_kind(&loaded.kind),
        "unrecognised kinds must not be treated as core"
    );
}

#[test]
fn manifest_parts_table_round_trips_through_write_manifest_and_read() {
    let (_dir, ticket_path) = make_ticket_dir();
    let mut manifest = TicketFs::read(&ticket_path).unwrap();

    let objective = TicketPart::new("objective", "parts/objective.md");
    let mut amendment = TicketPart::new("amendment", "parts/amend-1.md");
    amendment.supersedes = Some(objective.id);
    amendment.frozen = false;
    manifest.set_parts(vec![objective.clone(), amendment.clone()]);

    // Exercise the exact write path `TicketFs` uses: format then write.
    std::fs::write(
        ticket_path.join("ticket.toml"),
        crate::model::manifest_format::format_manifest_toml(&manifest),
    )
    .unwrap();

    let reloaded = TicketFs::read(&ticket_path).unwrap();
    let parts = reloaded.parts();
    assert_eq!(parts.len(), 2, "the [[parts]] table must not be dropped");
    assert_eq!(parts[0], objective);
    assert_eq!(parts[1], amendment);
    assert_eq!(parts[1].supersedes, Some(objective.id));
}

#[test]
fn ticket_fs_update_preserves_parts_table_when_patching_other_fields() {
    let (_dir, ticket_path) = make_ticket_dir();
    let mut manifest = TicketFs::read(&ticket_path).unwrap();
    let objective = TicketPart::new("objective", "parts/objective.md");
    manifest.set_parts(vec![objective.clone()]);
    std::fs::write(
        ticket_path.join("ticket.toml"),
        crate::model::manifest_format::format_manifest_toml(&manifest),
    )
    .unwrap();

    let mut patch = BTreeMap::new();
    patch.insert(
        "priority".to_string(),
        Value::String("high".to_string()),
    );
    let updated = TicketFs::update(&ticket_path, &patch, None).unwrap();

    assert_eq!(
        updated.parts(),
        vec![objective],
        "an unrelated field patch must not drop the [[parts]] table"
    );
}

// ── typed [[refs]] (spec 24b3d22b, ticket 9d69e93d) ──────────────────────────

#[test]
fn write_ref_appends_and_round_trips_all_six_kinds() {
    let (_dir, ticket_path) = make_ticket_dir();

    let entries = vec![
        ("spec", format!("ce://default/spec/{}", uuid::Uuid::new_v4()), Some("contract".to_string())),
        ("test_execution", "ce://default/test-execution/7f2c1a04".to_string(), None),
        ("log", "ce://default/log/build-2026-07-30".to_string(), None),
        ("rule", format!("ce://default/rule/{}", uuid::Uuid::new_v4()), None),
        ("file", "memory-api/crates/ticket-api/src/storage/store.rs".to_string(), Some("write path".to_string())),
        ("commit", "abc1234".to_string(), None),
    ];

    for (kind, urn, note) in &entries {
        TicketFs::write_ref(&ticket_path, kind, urn, note.clone()).unwrap();
    }

    let manifest = TicketFs::read(&ticket_path).unwrap();
    let refs = manifest.refs();
    assert_eq!(refs.len(), 6, "all six ref kinds must round-trip without loss");
    for (kind, urn, note) in &entries {
        let found = refs
            .iter()
            .find(|r| r.kind == *kind && r.urn == *urn)
            .unwrap_or_else(|| panic!("missing ref kind={kind} urn={urn}"));
        assert_eq!(found.note, *note);
    }
}

#[test]
fn write_ref_rejects_unknown_kind_with_vocabulary_in_error() {
    let (_dir, ticket_path) = make_ticket_dir();
    let err = TicketFs::write_ref(&ticket_path, "doc", "anything", None)
        .expect_err("unknown ref kind must be rejected");
    let message = err.to_string();
    assert!(message.contains("doc"));
    for kind in crate::model::refs::REF_KINDS {
        assert!(message.contains(kind), "{message}");
    }
    // Rejected before touching the manifest on disk.
    let manifest = TicketFs::read(&ticket_path).unwrap();
    assert!(manifest.refs().is_empty());
}

#[test]
fn write_ref_rejects_malformed_urn_for_kind() {
    let (_dir, ticket_path) = make_ticket_dir();

    assert!(TicketFs::write_ref(&ticket_path, "spec", "not-a-urn", None).is_err());
    assert!(TicketFs::write_ref(&ticket_path, "commit", "zz", None).is_err());
    assert!(TicketFs::write_ref(&ticket_path, "file", "/etc/passwd", None).is_err());

    let manifest = TicketFs::read(&ticket_path).unwrap();
    assert!(manifest.refs().is_empty(), "no malformed ref should persist");
}

#[test]
fn foreign_ref_kind_already_in_manifest_round_trips_unchanged() {
    let (_dir, ticket_path) = make_ticket_dir();
    let mut manifest = TicketFs::read(&ticket_path).unwrap();

    // Simulates a manifest written by a future/foreign tool with a ref kind
    // outside today's closed vocabulary. Reading must never drop or fail on
    // it, even though writing a *new* ref of this kind would be rejected.
    manifest.set_refs(vec![TicketRefEntry {
        kind: "future_kind".to_string(),
        urn: "ce://default/future_kind/whatever".to_string(),
        note: Some("preserved, not dropped".to_string()),
    }]);
    std::fs::write(
        ticket_path.join("ticket.toml"),
        crate::model::manifest_format::format_manifest_toml(&manifest),
    )
    .unwrap();

    let reloaded = TicketFs::read(&ticket_path).unwrap();
    let refs = reloaded.refs();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].kind, "future_kind");
    assert_eq!(refs[0].note, Some("preserved, not dropped".to_string()));
}

#[test]
fn legacy_related_specs_extra_bridges_to_refs_with_identical_spec_identity() {
    let (_dir, ticket_path) = make_ticket_dir();
    let mut manifest = TicketFs::read(&ticket_path).unwrap();

    let spec_id = uuid::Uuid::new_v4();
    manifest.set_related_specs(vec![SpecRef {
        spec_id,
        workspace: "default".to_string(),
        store_root: ".spec".to_string(),
    }]);
    std::fs::write(
        ticket_path.join("ticket.toml"),
        crate::model::manifest_format::format_manifest_toml(&manifest),
    )
    .unwrap();

    // No [[refs]] table was ever written for this ticket.
    let reloaded = TicketFs::read(&ticket_path).unwrap();
    assert!(reloaded.extra.get("refs").is_none());

    let refs = reloaded.refs();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].kind, "spec");
    assert_eq!(refs[0].urn, format!("ce://default/spec/{spec_id}"));

    // Diff of resolved spec ids before (legacy `related_specs()`) and after
    // (typed `refs()`) must be empty.
    let legacy_ids: Vec<uuid::Uuid> =
        reloaded.related_specs().iter().map(|r| r.spec_id).collect();
    let refs_ids: Vec<uuid::Uuid> = refs
        .iter()
        .filter(|r| r.kind == "spec")
        .map(|r| {
            r.urn
                .rsplit('/')
                .next()
                .unwrap()
                .parse::<uuid::Uuid>()
                .unwrap()
        })
        .collect();
    assert_eq!(legacy_ids, refs_ids);
}

#[test]
fn remove_ref_deletes_matching_entry_and_leaves_others() {
    let (_dir, ticket_path) = make_ticket_dir();
    TicketFs::write_ref(&ticket_path, "commit", "abc1234", None).unwrap();
    TicketFs::write_ref(&ticket_path, "commit", "def5678", None).unwrap();

    TicketFs::remove_ref(&ticket_path, "commit", "abc1234").unwrap();

    let manifest = TicketFs::read(&ticket_path).unwrap();
    let refs = manifest.refs();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].urn, "def5678");
}

#[test]
fn ticket_fs_update_preserves_refs_table_when_patching_other_fields() {
    let (_dir, ticket_path) = make_ticket_dir();
    TicketFs::write_ref(&ticket_path, "commit", "abc1234", None).unwrap();

    let mut patch = BTreeMap::new();
    patch.insert("priority".to_string(), Value::String("high".to_string()));
    let updated = TicketFs::update(&ticket_path, &patch, None).unwrap();

    assert_eq!(updated.refs().len(), 1, "an unrelated field patch must not drop [[refs]]");
}
