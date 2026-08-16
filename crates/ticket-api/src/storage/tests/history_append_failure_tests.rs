use super::*;
use std::sync::{
    Arc,
    Mutex,
};

// Regression coverage for the append_history "swallow" fix: every call site
// wraps `TicketFs::append_history` in `if let Err(error) = ... {
// tracing::error!(...) }` instead of `let _ = ...`. These tests force the
// history write to fail (by making `history.ndjson` read-only after any
// prerequisite reads succeed) and assert both that the surrounding
// operation still returns `Ok` (the manifest write is the system of
// record) and that the failure is actually logged rather than discarded.

#[derive(Clone, Default)]
struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CapturingWriter {
    fn write(
        &mut self,
        buf: &[u8],
    ) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturingWriter {
    type Writer = CapturingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn capture_tracing<F: FnOnce()>(f: F) -> String {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = CapturingWriter(buf.clone());
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .finish();
    tracing::subscriber::with_default(subscriber, f);
    let bytes = buf.lock().unwrap().clone();
    String::from_utf8(bytes).unwrap()
}

fn make_readonly(path: &Path) {
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(path, perms).unwrap();
}

fn make_writable(path: &Path) {
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_readonly(false);
    fs::set_permissions(path, perms).unwrap();
}

#[test]
fn create_logs_error_when_initial_history_append_fails() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let id = Uuid::new_v4();

    // Pre-seed the ticket's staging directory with a read-only
    // `history.ndjson` so that once `TicketFs::create` renames it into
    // place, the initial history append inside `store.create` fails.
    let tickets_root = store.index_root.join("tickets");
    let temp_dir = tickets_root.join(format!("{id}.tmp"));
    fs::create_dir_all(&temp_dir).unwrap();
    let staged_history = temp_dir.join("history.ndjson");
    fs::write(&staged_history, "").unwrap();
    make_readonly(&staged_history);

    let logs = capture_tracing(|| {
        let created = store.create(
            Some(id),
            "tracker-improvement",
            Some("Initial history append failure"),
            Some("planned"),
            Default::default(),
            None,
            None,
        );
        assert!(
            created.is_ok(),
            "ticket create must succeed even when the initial history \
             append fails: {created:?}"
        );
    });

    assert!(
        logs.contains("failed to append history revision"),
        "expected the swallowed history-append error to be logged, got: \
         {logs}"
    );

    make_writable(&tickets_root.join(id.to_string()).join("history.ndjson"));
}

#[test]
fn write_part_logs_error_when_history_append_fails() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Part write history failure"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let part_id = Uuid::new_v4();
    store
        .write_part(&id, part_id, "objective", "v1", None)
        .unwrap();

    let history_path = store
        .get_indexed(&id)
        .unwrap()
        .unwrap()
        .path
        .join("history.ndjson");
    make_readonly(&history_path);

    let logs = capture_tracing(|| {
        let result = store.write_part(&id, part_id, "objective", "v2", None);
        assert!(
            result.is_ok(),
            "write_part must succeed even when history append fails: \
             {result:?}"
        );
    });

    make_writable(&history_path);

    assert!(
        logs.contains("failed to append history revision"),
        "expected the swallowed history-append error to be logged, got: \
         {logs}"
    );
}

#[test]
fn write_amendment_part_logs_error_when_history_append_fails() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Amendment history failure"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let target_part_id = Uuid::new_v4();
    store
        .write_part(&id, target_part_id, "objective", "v1", None)
        .unwrap();

    let history_path = store
        .get_indexed(&id)
        .unwrap()
        .unwrap()
        .path
        .join("history.ndjson");
    make_readonly(&history_path);

    let amendment_part_id = Uuid::new_v4();
    let logs = capture_tracing(|| {
        let result = store.write_amendment_part(
            &id,
            amendment_part_id,
            "correction",
            target_part_id,
            None,
        );
        assert!(
            result.is_ok(),
            "write_amendment_part must succeed even when history append \
             fails: {result:?}"
        );
    });

    make_writable(&history_path);

    assert!(
        logs.contains("failed to append history revision"),
        "expected the swallowed history-append error to be logged, got: \
         {logs}"
    );
}

#[test]
fn undo_part_logs_error_when_history_append_fails() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Undo history failure"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let part_id = Uuid::new_v4();
    // Two writes so the second has prior content for undo to restore.
    store
        .write_part(&id, part_id, "objective", "v1", None)
        .unwrap();
    store
        .write_part(&id, part_id, "objective", "v2", None)
        .unwrap();

    let history_path = store
        .get_indexed(&id)
        .unwrap()
        .unwrap()
        .path
        .join("history.ndjson");
    make_readonly(&history_path);

    let logs = capture_tracing(|| {
        let result = store.undo_part(&id, part_id, None);
        assert!(
            result.is_ok(),
            "undo_part must succeed even when history append fails: \
             {result:?}"
        );
    });

    make_writable(&history_path);

    assert!(
        logs.contains("failed to append history revision"),
        "expected the swallowed history-append error to be logged, got: \
         {logs}"
    );
}

#[test]
fn attach_logs_error_when_history_append_fails() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Attach history failure"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let source_path = dir.path().join("attachment.txt");
    fs::write(&source_path, "attachment content").unwrap();

    let history_path = store
        .get_indexed(&id)
        .unwrap()
        .unwrap()
        .path
        .join("history.ndjson");
    make_readonly(&history_path);

    let logs = capture_tracing(|| {
        let result = store.attach(&id, &source_path, Some("attachment.txt"));
        assert!(
            result.is_ok(),
            "attach must succeed even when history append fails: {result:?}"
        );
    });

    make_writable(&history_path);

    assert!(
        logs.contains("failed to append history revision"),
        "expected the swallowed history-append error to be logged, got: \\n         {logs}"
    );
}

#[test]
fn add_edge_logs_error_when_history_append_fails() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let source = store
        .create(
            None,
            "tracker-improvement",
            Some("Source ticket"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let target = store
        .create(
            None,
            "tracker-improvement",
            Some("Target ticket"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let history_path = store
        .get_indexed(&source)
        .unwrap()
        .unwrap()
        .path
        .join("history.ndjson");
    make_readonly(&history_path);

    let logs = capture_tracing(|| {
        let result = store.add_edge(EdgeRecord {
            from: source,
            to: target,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        });
        assert!(
            result.is_ok(),
            "add_edge must succeed even when history append fails: {result:?}"
        );
    });

    make_writable(&history_path);

    assert!(
        logs.contains("failed to append history revision"),
        "expected the swallowed history-append error to be logged, got: \\n         {logs}"
    );
}

#[test]
fn remove_edge_logs_error_when_history_append_fails() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let source = store
        .create(
            None,
            "tracker-improvement",
            Some("Source ticket"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let target = store
        .create(
            None,
            "tracker-improvement",
            Some("Target ticket"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store
        .add_edge(EdgeRecord {
            from: source,
            to: target,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    let history_path = store
        .get_indexed(&source)
        .unwrap()
        .unwrap()
        .path
        .join("history.ndjson");
    make_readonly(&history_path);

    let logs = capture_tracing(|| {
        let result = store.remove_edge(EdgeRecord {
            from: source,
            to: target,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        });
        assert!(
            result.is_ok(),
            "remove_edge must succeed even when history append fails: \\n             {result:?}"
        );
    });

    make_writable(&history_path);

    assert!(
        logs.contains("failed to append history revision"),
        "expected the swallowed history-append error to be logged, got: \\n         {logs}"
    );
}
