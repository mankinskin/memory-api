use notify::{
    Event,
    EventKind,
    RecommendedWatcher,
    RecursiveMode,
    Watcher,
};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

use crate::{
    error::StorageError,
    model::filesystem::ParseDiagnostic,
    storage::store::{
        ScanReport,
        TicketStore,
    },
    watcher::events::WatchEventKind,
};

const WATCHER_TRACE_TARGET: &str = "ticket_api::watcher::reconciler";

/// A structured event emitted by the reconciler for external consumers.
pub struct ReconcileEvent {
    pub path: PathBuf,
    pub kind: WatchEventKind,
}

/// Perform a one-shot reconciliation pass over all scan roots.
/// This is the command-line equivalent of `ticket scan`.
pub fn reconcile_once(
    store: &TicketStore,
    reindex: bool,
) -> Result<ScanReport, StorageError> {
    let _span_guard = tracing::info_span!(
        target: WATCHER_TRACE_TARGET,
        "ticket_reconcile_once",
        reindex,
    )
    .entered();
    let report = store.scan(reindex)?;
    tracing::info!(
        target: WATCHER_TRACE_TARGET,
        reindex,
        integrated = report.integrated,
        pruned = report.pruned,
        diagnostics = report.diagnostics.len(),
        "ticket_reconcile_once_complete"
    );
    Ok(report)
}

/// Start an asynchronous filesystem watcher over all registered scan roots.
///
/// Returns a `WatchHandle` that keeps the watcher alive. Drop it to stop watching.
///
/// On each file change, triggers a targeted reconcile for the affected ticket folder.
/// Falls back to `scan()` for events that cannot be mapped to a specific ticket.
///
/// # Note
/// This is a best-effort watching layer. Crash safety and correctness are
/// guaranteed by `ticket scan --reindex`, not by the watcher alone.
pub fn start_watcher(store: &TicketStore) -> Result<WatchHandle, StorageError> {
    let roots = store.list_scan_roots()?;
    let default_root = store.index_root.join("tickets");

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = Watcher::new(
        tx,
        notify::Config::default().with_poll_interval(Duration::from_secs(2)),
    )
    .map_err(|e| {
        StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;

    // Watch default root.
    if default_root.exists() {
        let _ = watcher.watch(&default_root, RecursiveMode::Recursive);
    }
    // Watch all registered scan roots.
    for root in &roots {
        if root.path.exists() {
            let _ = watcher.watch(&root.path, RecursiveMode::Recursive);
        }
    }

    Ok(WatchHandle {
        _watcher: watcher,
        rx,
    })
}

/// Opaque handle that keeps the filesystem watcher alive.
/// Drop to stop watching.
pub struct WatchHandle {
    _watcher: RecommendedWatcher,
    pub rx: mpsc::Receiver<notify::Result<Event>>,
}

impl WatchHandle {
    /// Poll for the next event with a timeout.
    /// Returns `None` when the channel is idle.
    pub fn try_recv_event(&self) -> Option<notify::Result<Event>> {
        self.rx.try_recv().ok()
    }
}

/// Classify a `notify::Event` into our `WatchEventKind`.
pub fn classify_event(event: &Event) -> WatchEventKind {
    match event.kind {
        EventKind::Create(_) => WatchEventKind::Created,
        EventKind::Modify(_) => WatchEventKind::Modified,
        EventKind::Remove(_) => WatchEventKind::Deleted,
        EventKind::Access(_) => WatchEventKind::Modified,
        _ => WatchEventKind::Modified,
    }
}

/// Run a blocking watch loop that reconciles on filesystem events.
///
/// This function blocks the calling thread indefinitely.  It polls the
/// `WatchHandle` receiver, debounces events into batches, and calls
/// `integrate_orphan` for specifically identified ticket paths or falls back
/// to `reconcile_once` for unclassified events.
///
/// `debounce_ms` — how long to wait for additional events before triggering a
/// reconcile pass (default: 200ms is a sensible starting point).
///
/// Returns only if the watcher channel closes (which happens when the OS
/// reports a fatal error).
pub fn run_watch_loop(
    handle: &WatchHandle,
    store: &TicketStore,
    debounce_ms: u64,
) {
    use std::time::{
        Duration,
        Instant,
    };

    let debounce = Duration::from_millis(debounce_ms);
    let mut pending_paths: Vec<std::path::PathBuf> = Vec::new();
    let mut last_event: Option<Instant> = None;

    let _span_guard = tracing::info_span!(
        target: WATCHER_TRACE_TARGET,
        "ticket_watch_loop",
        debounce_ms,
    )
    .entered();

    loop {
        // Poll for new events.
        match handle.try_recv_event() {
            Some(Ok(event)) => {
                tracing::debug!(
                    target: WATCHER_TRACE_TARGET,
                    path_count = event.paths.len(),
                    kind = ?event.kind,
                    "ticket_watch_event_received"
                );
                for path in event.paths {
                    pending_paths.push(path);
                }
                last_event = Some(Instant::now());
            },
            Some(Err(error)) => {
                tracing::warn!(
                    target: WATCHER_TRACE_TARGET,
                    error = %error,
                    "ticket_watch_event_error"
                );
                // Watcher error — fall through to debounce-check.
            },
            None => {
                // No event right now — check if the debounce window has elapsed.
            },
        }

        // Check if we have pending events and the debounce window has elapsed.
        if let Some(ts) = last_event {
            if ts.elapsed() >= debounce && !pending_paths.is_empty() {
                let outcome = reconcile_pending_paths(store, &pending_paths);
                tracing::info!(
                    target: WATCHER_TRACE_TARGET,
                    pending_paths = pending_paths.len(),
                    targeted_paths = outcome.targeted_paths,
                    integrated_orphans = outcome.integrated_orphans,
                    fallback_scan = outcome.fallback_scan,
                    diagnostic_count = outcome.diagnostics.len(),
                    "ticket_watch_batch_reconciled"
                );

                pending_paths.clear();
                last_event = None;
            }
        }

        // Sleep briefly to avoid busy-looping.
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[derive(Debug, Default)]
struct ReconcileBatchOutcome {
    targeted_paths: usize,
    integrated_orphans: usize,
    fallback_scan: bool,
    diagnostics: Vec<ParseDiagnostic>,
}

fn reconcile_pending_paths(
    store: &TicketStore,
    pending_paths: &[std::path::PathBuf],
) -> ReconcileBatchOutcome {
    let _span_guard = tracing::debug_span!(
        target: WATCHER_TRACE_TARGET,
        "ticket_reconcile_pending_paths",
        pending_paths = pending_paths.len(),
        targeted_paths = tracing::field::Empty,
    )
    .entered();

    let targeted: Vec<_> = pending_paths
        .iter()
        .filter_map(|p| {
            // Walk up to find the UUID-named directory inside a scan root.
            find_ticket_root(p)
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    tracing::Span::current().record("targeted_paths", targeted.len());

    if targeted.is_empty() {
        match reconcile_once(store, false) {
            Ok(report) => ReconcileBatchOutcome {
                fallback_scan: true,
                diagnostics: report.diagnostics,
                ..Default::default()
            },
            Err(error) => {
                tracing::warn!(
                    target: WATCHER_TRACE_TARGET,
                    error = %error,
                    "ticket_reconcile_pending_paths_full_scan_failed"
                );
                ReconcileBatchOutcome {
                    fallback_scan: true,
                    ..Default::default()
                }
            },
        }
    } else {
        let mut outcome = ReconcileBatchOutcome {
            targeted_paths: targeted.len(),
            ..Default::default()
        };
        for ticket_path in targeted {
            match store.integrate_orphan(&ticket_path) {
                Ok(true) => {
                    outcome.integrated_orphans += 1;
                },
                Ok(false) => {
                    tracing::debug!(
                        target: WATCHER_TRACE_TARGET,
                        ticket_path = %ticket_path.display(),
                        "ticket_reconcile_pending_paths_orphan_skipped"
                    );
                },
                Err(error) => {
                    tracing::warn!(
                        target: WATCHER_TRACE_TARGET,
                        ticket_path = %ticket_path.display(),
                        error = %error,
                        "ticket_reconcile_pending_paths_orphan_failed"
                    );
                },
            }
        }
        outcome
    }
}

/// Given a path reported by the notify watcher, find the ticket root directory.
///
/// A ticket root is a UUID-named directory directly under a scan root.
/// Walk up ancestor directories until we find one whose name parses as a UUID.
fn find_ticket_root(path: &std::path::Path) -> Option<std::path::PathBuf> {
    use uuid::Uuid;
    let mut current = path;
    loop {
        if let Some(name) = current.file_name().and_then(|n| n.to_str()) {
            if name.parse::<Uuid>().is_ok() {
                return Some(current.to_path_buf());
            }
        }
        current = current.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn find_ticket_root_returns_uuid_ancestor() {
        let dir = tempdir().unwrap();
        let ticket_dir =
            dir.path().join("123e4567-e89b-12d3-a456-426614174000");
        let nested = ticket_dir.join("child").join("ticket.toml");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();

        let found = find_ticket_root(&nested).unwrap();
        assert_eq!(found, ticket_dir);
    }

    #[test]
    fn reconcile_pending_paths_falls_back_to_full_scan_without_ticket_paths() {
        let dir = tempdir().unwrap();
        let store = TicketStore::init(dir.path()).unwrap();
        let pending = vec![dir.path().join("unrelated").join("notes.txt")];

        let outcome = reconcile_pending_paths(&store, &pending);

        assert!(outcome.fallback_scan);
        assert_eq!(outcome.targeted_paths, 0);
        assert_eq!(outcome.integrated_orphans, 0);
    }

    #[test]
    fn reconcile_pending_paths_targets_orphan_ticket_folder() {
        let dir = tempdir().unwrap();
        let store = TicketStore::init(dir.path()).unwrap();
        let ticket_id = store
            .create(
                None,
                "tracker-improvement",
                Some("watch me"),
                Some("planned"),
                Default::default(),
                None,
                None,
            )
            .unwrap();
        let ticket_path = store
            .get_indexed(&ticket_id)
            .unwrap()
            .unwrap()
            .path
            .join("ticket.toml");

        let outcome = reconcile_pending_paths(&store, &[ticket_path]);

        assert!(!outcome.fallback_scan);
        assert_eq!(outcome.targeted_paths, 1);
        assert_eq!(outcome.integrated_orphans, 1);
    }
}
