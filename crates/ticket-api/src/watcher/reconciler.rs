use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crate::error::StorageError;
use crate::storage::store::{ScanReport, TicketStore};
use crate::watcher::events::WatchEventKind;

/// A structured event emitted by the reconciler for external consumers.
pub struct ReconcileEvent {
    pub path: PathBuf,
    pub kind: WatchEventKind,
}

/// Perform a one-shot reconciliation pass over all scan roots.
/// This is the command-line equivalent of `ticket scan`.
pub fn reconcile_once(store: &TicketStore, reindex: bool) -> Result<ScanReport, StorageError> {
    store.scan(reindex)
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
pub fn start_watcher(
    store: &TicketStore,
) -> Result<WatchHandle, StorageError> {
    let roots = store.list_scan_roots()?;
    let default_root = store.index_root.join("tickets");

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher =
        Watcher::new(tx, notify::Config::default().with_poll_interval(Duration::from_secs(2)))
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

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

    Ok(WatchHandle { _watcher: watcher, rx })
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
pub fn run_watch_loop(handle: &WatchHandle, store: &TicketStore, debounce_ms: u64) {
    use std::time::{Duration, Instant};

    let debounce = Duration::from_millis(debounce_ms);
    let mut pending_paths: Vec<std::path::PathBuf> = Vec::new();
    let mut last_event: Option<Instant> = None;

    loop {
        // Poll for new events.
        match handle.try_recv_event() {
            Some(Ok(event)) => {
                for path in event.paths {
                    pending_paths.push(path);
                }
                last_event = Some(Instant::now());
            }
            Some(Err(_)) => {
                // Watcher error — fall through to debounce-check.
            }
            None => {
                // No event right now — check if the debounce window has elapsed.
            }
        }

        // Check if we have pending events and the debounce window has elapsed.
        if let Some(ts) = last_event {
            if ts.elapsed() >= debounce && !pending_paths.is_empty() {
                // Attempt targeted per-path integration, fall back to full scan.
                let targeted: Vec<_> = pending_paths
                    .iter()
                    .filter_map(|p| {
                        // Walk up to find the UUID-named directory inside a scan root.
                        find_ticket_root(p)
                    })
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                if targeted.is_empty() {
                    // No specific ticket paths could be identified — run a full scan.
                    let _ = reconcile_once(store, false);
                } else {
                    for ticket_path in targeted {
                        let _ = store.integrate_orphan(&ticket_path);
                    }
                }

                pending_paths.clear();
                last_event = None;
            }
        }

        // Sleep briefly to avoid busy-looping.
        std::thread::sleep(Duration::from_millis(50));
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
