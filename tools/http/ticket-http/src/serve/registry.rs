//! Workspace registry: maps workspace names to `TicketStore` instances.

use std::{
    collections::{
        HashMap,
        HashSet,
    },
    path::{
        Path,
        PathBuf,
    },
    sync::{
        Arc,
        Condvar,
        Mutex,
    },
};

use ticket_api::{
    error::StorageError,
    model::filesystem::TICKET_MANIFEST_FILE,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
    },
};
use uuid::Uuid;

/// A map from workspace name → lazily-opened `TicketStore`.
pub struct WorkspaceRegistry {
    /// Canonical name of the primary workspace served by this registry.
    primary_workspace: String,
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
        let registry = Arc::new(WorkspaceRegistry::single(dir.path().to_path_buf()));
        let primary_workspace = registry.primary_workspace_name().to_string();

        let workers = 8usize;
        let barrier = Arc::new(Barrier::new(workers));
        let mut handles = Vec::with_capacity(workers);

        for _ in 0..workers {
            let registry = Arc::clone(&registry);
            let primary_workspace = primary_workspace.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                registry.get(&primary_workspace).expect("workspace should open")
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
    /// Build with a single pre-loaded workspace named from its workspace folder.
    pub fn single(path: PathBuf) -> Self {
        let primary_workspace =
            primary_workspace_name_for_index_root(&path);
        let mut paths = HashMap::new();
        paths.insert(primary_workspace.clone(), path);
        Self {
            primary_workspace,
            paths,
            stores: Mutex::new(HashMap::new()),
            opening: Mutex::new(HashSet::new()),
            opening_cv: Condvar::new(),
        }
    }

    /// Build with a single already-open store named from its workspace folder.
    ///
    /// Use this when the caller already holds an open `TicketStore` to avoid a
    /// second open attempt on the same SQLite file (only one writer at a time).
    pub fn single_opened(store: Arc<TicketStore>) -> Self {
        let path = store.index_root.clone();
        let primary_workspace =
            primary_workspace_name_for_index_root(&path);
        let mut paths = HashMap::new();
        paths.insert(primary_workspace.clone(), path);
        extend_related_paths(&mut paths, &store);
        let mut stores = HashMap::new();
        stores.insert(primary_workspace.clone(), store);
        Self {
            primary_workspace,
            paths,
            stores: Mutex::new(stores),
            opening: Mutex::new(HashSet::new()),
            opening_cv: Condvar::new(),
        }
    }

    pub fn primary_workspace_name(&self) -> &str {
        &self.primary_workspace
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
        let mut workspace_names = self.workspace_names();
        if let Some(index) = workspace_names
            .iter()
            .position(|workspace| workspace == active_workspace)
        {
            let active = workspace_names.remove(index);
            workspace_names.insert(0, active);
        }

        for workspace in workspace_names {
            let Some(store) = self.get(&workspace) else {
                continue;
            };
            let canonical_workspace =
                canonical_workspace_name_for_index_root(&store.index_root, &workspace);
            let found = store.get_indexed_many(ids)?;
            for (id, ticket) in found {
                if ticket.deleted {
                    continue;
                }
                let candidate = ResolvedIndexedTicket {
                    workspace: canonical_workspace.clone(),
                    store: Arc::clone(&store),
                    ticket,
                };

                match resolved.get_mut(&id) {
                    Some(current)
                        if !prefer_resolved_ticket(
                            active_workspace,
                            current,
                            &candidate,
                        ) => {},
                    Some(current) => *current = candidate,
                    None => {
                        resolved.insert(id, candidate);
                    },
                }
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

fn prefer_resolved_ticket(
    active_workspace: &str,
    current: &ResolvedIndexedTicket,
    candidate: &ResolvedIndexedTicket,
) -> bool {
    let current_score = resolved_ticket_score(active_workspace, current);
    let candidate_score = resolved_ticket_score(active_workspace, candidate);
    candidate_score > current_score
}

fn resolved_ticket_score(
    active_workspace: &str,
    ticket: &ResolvedIndexedTicket,
) -> (bool, usize, bool) {
    (
        ticket.path_exists(),
        ticket.store.index_root.components().count(),
        ticket.workspace == active_workspace,
    )
}

impl ResolvedIndexedTicket {
    fn path_exists(&self) -> bool {
        self.ticket.path.join(TICKET_MANIFEST_FILE).is_file()
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
            let index_root = store_root_for_scan_root(&root.path)?;
            if index_root == store.index_root {
                return None;
            }
            Some((
                canonical_workspace_name_for_index_root(&index_root, &root.label),
                index_root,
            ))
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
            ancestors.push((
                canonical_workspace_name_for_index_root(
                    &candidate,
                    &ancestor_label(depth),
                ),
                candidate,
            ));
        }
        current = dir.parent();
        depth += 1;
    }

    ancestors
}

fn workspace_root_for_store(store: &TicketStore) -> &std::path::Path {
    workspace_root_for_index_root(&store.index_root)
}

pub(crate) fn workspace_root_for_index_root(
    index_root: &Path,
) -> &Path {
    match index_root.file_name().and_then(|name| name.to_str()) {
        Some(".ticket") => index_root.parent().unwrap_or(index_root),
        _ => index_root,
    }
}

pub(crate) fn canonical_workspace_name_for_index_root(
    index_root: &Path,
    fallback: &str,
) -> String {
    workspace_root_for_index_root(index_root)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn primary_workspace_name_for_index_root(index_root: &Path) -> String {
    canonical_workspace_name_for_index_root(index_root, "workspace")
}

pub(crate) fn store_root_for_scan_root(
    scan_root: &Path,
) -> Option<PathBuf> {
    let parent = scan_root.parent()?;
    detect_store_root(parent)
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

#[cfg(test)]
mod workspace_resolution_tests {
    use super::WorkspaceRegistry;
    use std::{
        collections::BTreeMap,
        sync::Arc,
    };
    use ticket_api::{
        model::filesystem::ScanRoot,
        storage::store::TicketStore,
    };

    #[test]
    fn descendant_workspaces_use_workspace_root_name() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent_store = Arc::new(TicketStore::init(root.path()).expect("open parent store"));
        parent_store
            .add_scan_root(ScanRoot {
                path: root.path().join("tickets"),
                label: "default".to_string(),
            })
            .expect("add parent scan root");

        let child_index_root = root.path().join("child").join(".ticket");
        std::fs::create_dir_all(child_index_root.join("tickets"))
            .expect("mkdir child store");
        let child_store = Arc::new(
            TicketStore::init(&child_index_root).expect("open child store"),
        );
        child_store
            .add_scan_root(ScanRoot {
                path: child_index_root.join("tickets"),
                label: "tickets".to_string(),
            })
            .expect("add child scan root");

        parent_store
            .add_scan_root(ScanRoot {
                path: child_index_root.join("tickets"),
                label: "tickets".to_string(),
            })
            .expect("add child scan root to parent");

        let registry = WorkspaceRegistry::single_opened(Arc::clone(&parent_store));
        let workspace_names = registry.workspace_names();
        assert!(workspace_names.contains(&"child".to_string()));
        let root_workspace = super::workspace_root_for_index_root(root.path())
            .file_name()
            .and_then(|name| name.to_str())
            .expect("root workspace name")
            .to_string();
        assert!(workspace_names.contains(&root_workspace));
        assert!(!workspace_names.contains(&"tickets".to_string()));
    }

    #[test]
    fn resolve_indexed_many_prefers_deepest_existing_workspace() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent_store = Arc::new(TicketStore::init(root.path()).expect("open parent store"));
        parent_store
            .add_scan_root(ScanRoot {
                path: root.path().join("tickets"),
                label: "default".to_string(),
            })
            .expect("add parent scan root");

        let child_index_root = root.path().join("child").join(".ticket");
        std::fs::create_dir_all(child_index_root.join("tickets"))
            .expect("mkdir child store");
        let child_store = Arc::new(
            TicketStore::init(&child_index_root).expect("open child store"),
        );
        child_store
            .add_scan_root(ScanRoot {
                path: child_index_root.join("tickets"),
                label: "tickets".to_string(),
            })
            .expect("add child scan root");

        let ticket_id = child_store
            .create(
                None,
                "tracker-improvement",
                Some("child-owned ticket"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create child ticket");

        parent_store
            .add_scan_root(ScanRoot {
                path: child_index_root.join("tickets"),
                label: "tickets".to_string(),
            })
            .expect("add child scan root to parent");
        parent_store.scan(true).expect("scan parent store");

        let registry = WorkspaceRegistry::single_opened(Arc::clone(&parent_store));
        let resolved = registry
            .resolve_indexed_many(registry.primary_workspace_name(), &[ticket_id])
            .expect("resolve ticket");
        let resolved = resolved.get(&ticket_id).expect("resolved ticket");

        assert_eq!(resolved.workspace, "child");
        assert!(resolved.ticket.path.join("ticket.toml").is_file());
    }
}
