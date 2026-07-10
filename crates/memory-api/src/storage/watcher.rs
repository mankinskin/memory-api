//! Generic filesystem watcher that keeps the search index in sync.
//!
//! Any change to an entity folder under a registered scan root triggers a
//! targeted re-integration of just that entity into the metadata index and the
//! Tantivy full-text search index. Deletions remove the entity from both
//! indexes. This is a best-effort layer: full correctness is still guaranteed
//! by a forced `scan(true)` reindex.

use std::{
    path::{
        Path,
        PathBuf,
    },
    sync::mpsc,
    time::{
        Duration,
        Instant,
    },
};

use notify::{
    Event,
    RecommendedWatcher,
    RecursiveMode,
    Watcher,
};
use uuid::Uuid;

use crate::{
    error::StorageError,
    storage::entity_store::EntityStore,
};

/// Opaque handle that keeps the filesystem watcher alive.
///
/// Drop it to stop watching.
pub struct WatchHandle {
    _watcher: RecommendedWatcher,
    pub rx: mpsc::Receiver<notify::Result<Event>>,
}

impl WatchHandle {
    /// Poll for the next event without blocking.
    ///
    /// Returns `None` when the channel is currently idle.
    pub fn try_recv_event(&self) -> Option<notify::Result<Event>> {
        self.rx.try_recv().ok()
    }
}

impl EntityStore {
    /// Start an asynchronous filesystem watcher over the default entities root
    /// and every registered scan root.
    ///
    /// Returns a [`WatchHandle`] that keeps the watcher alive; drop it to stop.
    /// Pair it with [`run_watch_loop`] to continuously reconcile changes, or
    /// poll [`WatchHandle::try_recv_event`] and call
    /// [`EntityStore::reconcile_path`] yourself.
    pub fn start_watcher(&self) -> Result<WatchHandle, StorageError> {
        let roots = self.list_scan_roots()?;
        let default_root = self.index_root.join("entities");

        let (tx, rx) = mpsc::channel();
        let mut watcher: RecommendedWatcher = Watcher::new(
            tx,
            notify::Config::default()
                .with_poll_interval(Duration::from_secs(2)),
        )
        .map_err(|e| {
            StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        if default_root.exists() {
            let _ = watcher.watch(&default_root, RecursiveMode::Recursive);
        }
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

    /// Reconcile a single path reported by the watcher.
    ///
    /// Walks up to the UUID-named entity folder containing `path` and either
    /// re-integrates it (when it still exists on disk) or removes it from the
    /// index + search (when the folder is gone).
    ///
    /// Returns `Ok(true)` when an entity was integrated or removed, `Ok(false)`
    /// when `path` does not belong to a recognizable entity folder.
    pub fn reconcile_path(
        &self,
        path: &Path,
    ) -> Result<bool, StorageError> {
        let Some(entity_root) = find_entity_root(path) else {
            return Ok(false);
        };

        if entity_root.exists() {
            self.integrate_orphan(&entity_root)
        } else if let Some(id) = entity_root
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<Uuid>().ok())
        {
            self.remove_entity(&id)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Run a blocking watch loop that reconciles the search index on filesystem
/// events.
///
/// Blocks the calling thread indefinitely. Events are debounced into batches:
/// after `debounce_ms` of quiet following the last event, each affected entity
/// folder is reconciled via [`EntityStore::reconcile_path`].
pub fn run_watch_loop(
    handle: &WatchHandle,
    store: &EntityStore,
    debounce_ms: u64,
) {
    let debounce = Duration::from_millis(debounce_ms);
    let mut pending_paths: Vec<PathBuf> = Vec::new();
    let mut last_event: Option<Instant> = None;

    loop {
        match handle.try_recv_event() {
            Some(Ok(event)) => {
                pending_paths.extend(event.paths);
                last_event = Some(Instant::now());
            },
            Some(Err(_)) | None => {},
        }

        if let Some(ts) = last_event {
            if ts.elapsed() >= debounce && !pending_paths.is_empty() {
                let targeted: Vec<PathBuf> = pending_paths
                    .iter()
                    .filter_map(|p| find_entity_root(p))
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                for entity_root in targeted {
                    let _ = store.reconcile_path(&entity_root);
                }

                pending_paths.clear();
                last_event = None;
            }
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Walk up ancestor directories from `path` until one is named by a UUID.
///
/// Entity folders are UUID-named directories directly under a scan root, so the
/// nearest UUID-named ancestor identifies the changed entity.
fn find_entity_root(path: &Path) -> Option<PathBuf> {
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
    use std::fs;

    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;
    use crate::{
        model::filesystem::{
            EntityFolderConfig,
            ScanRoot,
        },
        storage::entity_fs::EntityFs,
    };

    fn open_store(dir: &Path) -> EntityStore {
        let fs = EntityFs::with_config(
            EntityFolderConfig::new("entity.toml", ".lock")
                .with_body_file("body.md"),
        );
        let store = EntityStore::open(dir, fs).unwrap();
        store
            .add_scan_root(ScanRoot {
                path: dir.join("entities"),
                label: "default".into(),
            })
            .unwrap();
        store
    }

    fn write_entity(
        root: &Path,
        id: Uuid,
        title: &str,
        body: &str,
    ) -> PathBuf {
        let folder = root.join("entities").join(id.to_string());
        fs::create_dir_all(&folder).unwrap();
        let manifest = format!(
            "id = \"{id}\"\ntitle = \"{title}\"\ncreated_at = \"2024-01-01T00:00:00Z\"\n"
        );
        fs::write(folder.join("entity.toml"), manifest).unwrap();
        fs::write(folder.join("body.md"), body).unwrap();
        folder
    }

    #[test]
    fn reconcile_path_indexes_new_entity_for_search() {
        let dir = TempDir::new().unwrap();
        let store = open_store(dir.path());
        let id = Uuid::new_v4();
        let folder = write_entity(dir.path(), id, "Alpha", "needle haystack");

        let changed = store.reconcile_path(&folder.join("body.md")).unwrap();
        assert!(changed);

        let results = store.search("needle", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);
    }

    #[test]
    fn reconcile_path_removes_deleted_entity_from_search() {
        let dir = TempDir::new().unwrap();
        let store = open_store(dir.path());
        let id = Uuid::new_v4();
        let folder = write_entity(dir.path(), id, "Beta", "removable token");

        store.reconcile_path(&folder).unwrap();
        assert_eq!(store.search("removable", 10).unwrap().len(), 1);

        fs::remove_dir_all(&folder).unwrap();
        let changed = store.reconcile_path(&folder).unwrap();
        assert!(changed);

        assert!(store.search("removable", 10).unwrap().is_empty());
        assert!(store.get_indexed(&id).unwrap().is_none());
    }

    #[test]
    fn reconcile_path_ignores_non_entity_paths() {
        let dir = TempDir::new().unwrap();
        let store = open_store(dir.path());
        let stray = dir.path().join("not-a-uuid").join("file.txt");
        assert!(!store.reconcile_path(&stray).unwrap());
    }
}
