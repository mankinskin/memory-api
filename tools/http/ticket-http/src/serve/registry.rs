//! Workspace registry: maps workspace names to `TicketStore` instances.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};

use ticket_api::{
    storage::store::TicketStore,
    workspace::WorkspaceConfig,
};

/// A map from workspace name → lazily-opened `TicketStore`.
pub struct WorkspaceRegistry {
    /// name → filesystem path to the `.ticket/` index root.
    paths: HashMap<String, PathBuf>,
    /// Lazy-opened stores, keyed by name.
    stores: Mutex<HashMap<String, Arc<TicketStore>>>,
    /// Workspaces currently being opened by another thread.
    opening: Mutex<HashSet<String>>,
    /// Notifies waiters when a workspace open attempt completes.
    opening_cv: Condvar,
}

#[cfg(test)]
mod tests {
    use super::WorkspaceRegistry;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn concurrent_get_returns_shared_store_instance() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let registry = Arc::new(WorkspaceRegistry::single(dir.path().to_path_buf()));

        let workers = 8usize;
        let barrier = Arc::new(Barrier::new(workers));
        let mut handles = Vec::with_capacity(workers);

        for _ in 0..workers {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                registry
                    .get("default")
                    .expect("workspace should open")
            }));
        }

        let first = handles
            .remove(0)
            .join()
            .expect("thread should join without panic");

        for handle in handles {
            let store = handle.join().expect("thread should join without panic");
            assert!(
                Arc::ptr_eq(&first, &store),
                "all concurrent gets should return the same cached store instance"
            );
        }
    }
}

impl WorkspaceRegistry {
    /// Build from a `WorkspaceConfig` (reads `~/.ticket-workspaces.toml`).
    pub fn from_config(config: &WorkspaceConfig) -> Self {
        let paths = config
            .workspaces
            .iter()
            .map(|(name, path_str)| (name.clone(), PathBuf::from(path_str)))
            .collect();
        Self {
            paths,
            stores: Mutex::new(HashMap::new()),
            opening: Mutex::new(HashSet::new()),
            opening_cv: Condvar::new(),
        }
    }

    /// Build with a single pre-loaded workspace named `"default"`.
    pub fn single(path: PathBuf) -> Self {
        let mut paths = HashMap::new();
        paths.insert("default".into(), path);
        Self {
            paths,
            stores: Mutex::new(HashMap::new()),
            opening: Mutex::new(HashSet::new()),
            opening_cv: Condvar::new(),
        }
    }

    /// Build with a single already-open store named `"default"`.
    ///
    /// Use this when the caller already holds an open `TicketStore` to avoid a
    /// second open attempt on the same redb file (redb does not allow concurrent
    /// opens from the same process).
    pub fn single_opened(store: Arc<TicketStore>) -> Self {
        let path = store.index_root.clone();
        let mut paths = HashMap::new();
        paths.insert("default".into(), path);
        let mut stores = HashMap::new();
        stores.insert("default".into(), store);
        Self {
            paths,
            stores: Mutex::new(stores),
            opening: Mutex::new(HashSet::new()),
            opening_cv: Condvar::new(),
        }
    }

    /// List workspace names.
    pub fn workspace_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.paths.keys().cloned().collect();
        names.sort();
        names
    }

    /// Return `true` if a workspace with the given name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.paths.contains_key(name)
    }

    /// Get or lazily open the `TicketStore` for `workspace`.
    ///
    /// Returns `None` if the workspace name is not registered.
    pub fn get(&self, workspace: &str) -> Option<Arc<TicketStore>> {
        let path = self.paths.get(workspace)?.clone();

        {
            let stores = self.stores.lock().unwrap();
            if let Some(store) = stores.get(workspace) {
                return Some(Arc::clone(store));
            }
        }

        // Coordinate concurrent lazy opens: only one thread opens a given
        // workspace, others wait for the result and use the cached store.
        {
            let mut opening = self.opening.lock().unwrap();
            loop {
                if !opening.contains(workspace) {
                    opening.insert(workspace.to_string());
                    break;
                }
                opening = self.opening_cv.wait(opening).unwrap();
                if let Some(existing) = self.stores.lock().unwrap().get(workspace).cloned() {
                    return Some(existing);
                }
            }
        }

        // Lazy open outside mutexes to avoid blocking unrelated requests.
        let opened = match TicketStore::open(&path) {
            Ok(store) => Some(Arc::new(store)),
            Err(e) => {
                tracing::warn!(workspace, error = %e, "failed to open workspace store");
                None
            }
        };

        let result = {
            let mut stores = self.stores.lock().unwrap();
            if let Some(existing) = stores.get(workspace) {
                Some(Arc::clone(existing))
            } else if let Some(opened) = opened {
                stores.insert(workspace.to_string(), Arc::clone(&opened));
                Some(opened)
            } else {
                None
            }
        };

        let mut opening = self.opening.lock().unwrap();
        opening.remove(workspace);
        self.opening_cv.notify_all();

        if result.is_none() {
            if let Some(existing) = self.stores.lock().unwrap().get(workspace).cloned() {
                return Some(existing);
            }
        }

        result
    }
}
