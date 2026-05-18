//! Workspace registry: maps workspace names to `TicketStore` instances.

use std::{
    collections::{
        HashMap,
        HashSet,
    },
    path::PathBuf,
    sync::{
        Arc,
        Condvar,
        Mutex,
    },
};

use ticket_api::{
    error::StorageError,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
    },
};
use uuid::Uuid;

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

#[derive(Clone)]
pub struct ResolvedIndexedTicket {
    pub workspace: String,
    pub store: Arc<TicketStore>,
    pub ticket: IndexedTicket,
}

#[cfg(test)]
mod tests {
    use super::WorkspaceRegistry;
    use std::{
        sync::{
            Arc,
            Barrier,
        },
        thread,
    };

    #[test]
    fn concurrent_get_returns_shared_store_instance() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let registry =
            Arc::new(WorkspaceRegistry::single(dir.path().to_path_buf()));

        let workers = 8usize;
        let barrier = Arc::new(Barrier::new(workers));
        let mut handles = Vec::with_capacity(workers);

        for _ in 0..workers {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                registry.get("default").expect("workspace should open")
            }));
        }

        let first = handles
            .remove(0)
            .join()
            .expect("thread should join without panic");

        for handle in handles {
            let store =
                handle.join().expect("thread should join without panic");
            assert!(
                Arc::ptr_eq(&first, &store),
                "all concurrent gets should return the same cached store instance"
            );
        }
    }
}

impl WorkspaceRegistry {
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
    /// second open attempt on the same SQLite file (only one writer at a time).
    pub fn single_opened(store: Arc<TicketStore>) -> Self {
        let path = store.index_root.clone();
        let mut paths = HashMap::new();
        paths.insert("default".into(), path);
        extend_related_paths(&mut paths, &store);
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

    pub fn resolve_indexed_many(
        &self,
        active_workspace: &str,
        ids: &[Uuid],
    ) -> Result<HashMap<Uuid, ResolvedIndexedTicket>, StorageError> {
        let mut resolved = HashMap::new();
        let mut remaining = ids.to_vec();
        let mut workspace_names = self.workspace_names();
        if let Some(index) = workspace_names
            .iter()
            .position(|workspace| workspace == active_workspace)
        {
            let active = workspace_names.remove(index);
            workspace_names.insert(0, active);
        }

        for workspace in workspace_names {
            if remaining.is_empty() {
                break;
            }

            let Some(store) = self.get(&workspace) else {
                continue;
            };
            let found = store.get_indexed_many(&remaining)?;
            remaining.retain(|id| !found.contains_key(id));
            for (id, ticket) in found {
                if ticket.deleted {
                    continue;
                }
                resolved.entry(id).or_insert_with(|| ResolvedIndexedTicket {
                    workspace: workspace.clone(),
                    store: Arc::clone(&store),
                    ticket,
                });
            }
        }

        Ok(resolved)
    }

    /// Return `true` if a workspace with the given name is registered.
    pub fn contains(
        &self,
        name: &str,
    ) -> bool {
        self.paths.contains_key(name)
    }

    /// Get or lazily open the `TicketStore` for `workspace`.
    ///
    /// Returns `None` if the workspace name is not registered.
    pub fn get(
        &self,
        workspace: &str,
    ) -> Option<Arc<TicketStore>> {
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
                if let Some(existing) =
                    self.stores.lock().unwrap().get(workspace).cloned()
                {
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
            },
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
            if let Some(existing) =
                self.stores.lock().unwrap().get(workspace).cloned()
            {
                return Some(existing);
            }
        }

        result
    }
}

fn extend_related_paths(
    paths: &mut HashMap<String, PathBuf>,
    store: &TicketStore,
) {
    for (name, path) in discover_descendant_workspace_paths(store) {
        paths.entry(name).or_insert(path);
    }
    for (name, path) in discover_ancestor_workspace_paths(store) {
        paths.entry(name).or_insert(path);
    }
}

fn discover_descendant_workspace_paths(
    store: &TicketStore,
) -> Vec<(String, PathBuf)> {
    let Ok(scan_roots) = store.list_scan_roots() else {
        return Vec::new();
    };

    scan_roots
        .into_iter()
        .filter_map(|root| {
            let index_root = root.path.parent()?.to_path_buf();
            if index_root == store.index_root {
                return None;
            }
            Some((root.label, index_root))
        })
        .collect()
}

fn discover_ancestor_workspace_paths(
    store: &TicketStore,
) -> Vec<(String, PathBuf)> {
    let active_workspace_root = workspace_root_for_store(store);

    let mut current = active_workspace_root.parent();
    let mut depth = 1usize;
    let mut ancestors = Vec::new();

    while let Some(dir) = current {
        if let Some(candidate) = detect_store_root(dir) {
            ancestors.push((ancestor_label(depth), candidate));
        }
        current = dir.parent();
        depth += 1;
    }

    ancestors
}

fn workspace_root_for_store(store: &TicketStore) -> &std::path::Path {
    match store.index_root.file_name().and_then(|name| name.to_str()) {
        Some(".ticket") => store.index_root.parent().unwrap_or(&store.index_root),
        _ => &store.index_root,
    }
}

fn detect_store_root(dir: &std::path::Path) -> Option<PathBuf> {
    if dir.join("tickets.db").is_file() {
        return Some(dir.to_path_buf());
    }

    let hidden = dir.join(".ticket");
    if hidden.join("tickets.db").is_file() {
        return Some(hidden);
    }

    None
}

fn ancestor_label(depth: usize) -> String {
    std::iter::repeat("..")
        .take(depth)
        .collect::<Vec<_>>()
        .join("/")
}
