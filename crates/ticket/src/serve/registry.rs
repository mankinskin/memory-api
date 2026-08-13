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
    /// public workspace id → filesystem path and display label.
    workspaces: HashMap<String, WorkspaceEntry>,
    /// Legacy nested path aliases → canonical workspace id.
    legacy_aliases: HashMap<String, String>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceNameInfo {
    pub name: String,
    pub label: String,
}

#[derive(Clone)]
struct WorkspaceEntry {
    path: PathBuf,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceResolveError {
    DisplayLabelRejected {
        requested: String,
        canonical: String,
    },
    AmbiguousLegacyLabel {
        requested: String,
        matches: Vec<String>,
    },
}

#[cfg(test)]
#[path = "registry/tests.rs"]
mod tests;

impl WorkspaceRegistry {
    /// Build with a single pre-loaded workspace named from its workspace folder.
    pub fn single(path: PathBuf) -> Self {
        let primary_workspace = primary_workspace_name_for_index_root(&path);
        let mut workspaces = HashMap::new();
        workspaces.insert(
            primary_workspace.clone(),
            WorkspaceEntry {
                path: path.clone(),
                label: workspace_label_for_index_root(&path, "workspace"),
            },
        );
        Self {
            primary_workspace,
            workspaces,
            legacy_aliases: HashMap::new(),
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
        let primary_workspace = primary_workspace_name_for_index_root(&path);
        let mut workspaces = HashMap::new();
        workspaces.insert(
            primary_workspace.clone(),
            WorkspaceEntry {
                path: path.clone(),
                label: workspace_label_for_index_root(&path, "workspace"),
            },
        );
        let mut legacy_aliases = HashMap::new();
        extend_related_paths(&mut workspaces, &mut legacy_aliases, &store);
        let mut stores = HashMap::new();
        stores.insert(primary_workspace.clone(), store);
        Self {
            primary_workspace,
            workspaces,
            legacy_aliases,
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
        let mut names: Vec<_> = self.workspaces.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn workspace_infos(&self) -> Vec<WorkspaceNameInfo> {
        let mut infos: Vec<_> = self
            .workspaces
            .iter()
            .map(|(name, entry)| WorkspaceNameInfo {
                name: name.clone(),
                label: entry.label.clone(),
            })
            .collect();
        infos.sort_by(|left, right| left.name.cmp(&right.name));
        infos
    }

    pub fn resolve_workspace_name(
        &self,
        workspace: &str,
    ) -> Result<Option<String>, WorkspaceResolveError> {
        if self.workspaces.contains_key(workspace) {
            return Ok(Some(workspace.to_string()));
        }

        if let Some(workspace) = self.legacy_aliases.get(workspace) {
            return Ok(Some(workspace.clone()));
        }

        let mut matches = self
            .workspaces
            .iter()
            .filter_map(|(name, entry)| {
                (entry.label == workspace).then(|| name.clone())
            })
            .collect::<Vec<_>>();
        matches.sort();

        match matches.len() {
            0 => Ok(None),
            1 => Err(WorkspaceResolveError::DisplayLabelRejected {
                requested: workspace.to_string(),
                canonical: matches.into_iter().next().unwrap(),
            }),
            _ => Err(WorkspaceResolveError::AmbiguousLegacyLabel {
                requested: workspace.to_string(),
                matches,
            }),
        }
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
            let canonical_workspace = canonical_workspace_name_for_index_root(
                &store.index_root,
                &workspace,
            );
            let found = store.get_indexed_many(ids)?;
            for (id, ticket) in found {
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
        self.workspaces.contains_key(name)
    }

    /// Get or lazily open the `TicketStore` for `workspace`.
    ///
    /// Returns `None` if the workspace name is not registered.
    pub fn get(
        &self,
        workspace: &str,
    ) -> Option<Arc<TicketStore>> {
        let path = self.workspaces.get(workspace)?.path.clone();

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
    workspaces: &mut HashMap<String, WorkspaceEntry>,
    legacy_aliases: &mut HashMap<String, String>,
    store: &TicketStore,
) {
    for (name, entry, alias) in discover_descendant_workspace_paths(store) {
        let canonical_name = name.clone();
        workspaces.entry(name).or_insert(entry);
        if let Some(alias) = alias {
            legacy_aliases.entry(alias).or_insert(canonical_name);
        }
    }
    for (name, entry) in discover_ancestor_workspace_paths(store) {
        workspaces.entry(name).or_insert(entry);
    }
}

fn discover_descendant_workspace_paths(
    store: &TicketStore
) -> Vec<(String, WorkspaceEntry, Option<String>)> {
    let Ok(scan_roots) = store.list_scan_roots() else {
        return Vec::new();
    };

    let active_workspace_root = workspace_root_for_store(store).to_path_buf();

    scan_roots
        .into_iter()
        .filter_map(|root| {
            let index_root = store_root_for_scan_root(&root.path)?;
            if index_root == store.index_root {
                return None;
            }
            let label =
                workspace_label_for_index_root(&index_root, &root.label);
            Some((
                canonical_workspace_name_for_index_root(
                    &index_root,
                    &root.label,
                ),
                WorkspaceEntry {
                    path: index_root,
                    label,
                },
                legacy_path_alias(&active_workspace_root, &root.path),
            ))
        })
        .collect()
}

fn legacy_path_alias(
    active_workspace_root: &Path,
    scan_root: &Path,
) -> Option<String> {
    let store_root = store_root_for_scan_root(scan_root)?;
    let workspace_root = workspace_root_for_index_root(&store_root);
    let relative = workspace_root
        .strip_prefix(active_workspace_root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");

    relative.contains('/').then_some(relative)
}

fn discover_ancestor_workspace_paths(
    store: &TicketStore
) -> Vec<(String, WorkspaceEntry)> {
    let active_workspace_root = workspace_root_for_store(store);

    let mut current = active_workspace_root.parent();
    let mut depth = 1usize;
    let mut ancestors = Vec::new();

    while let Some(dir) = current {
        if let Some(candidate) = detect_store_root(dir) {
            let fallback = ancestor_label(depth);
            let label = workspace_label_for_index_root(&candidate, &fallback);
            ancestors.push((
                canonical_workspace_name_for_index_root(&candidate, &fallback),
                WorkspaceEntry {
                    path: candidate,
                    label,
                },
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

pub(crate) fn workspace_root_for_index_root(index_root: &Path) -> &Path {
    match index_root.file_name().and_then(|name| name.to_str()) {
        Some(".ticket") => index_root.parent().unwrap_or(index_root),
        _ => index_root,
    }
}

pub(crate) fn canonical_workspace_name_for_index_root(
    index_root: &Path,
    fallback: &str,
) -> String {
    let label = workspace_label_for_index_root(index_root, fallback);
    format!(
        "{label}--{}",
        short_workspace_hash(workspace_root_for_index_root(index_root))
    )
}

pub(crate) fn workspace_label_for_index_root(
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

pub(crate) fn store_root_for_scan_root(scan_root: &Path) -> Option<PathBuf> {
    let parent = scan_root.parent()?;
    detect_store_root(parent)
}

fn detect_store_root(dir: &std::path::Path) -> Option<PathBuf> {
    if dir.join("tickets.db").is_file() || has_ticket_manifest(dir) {
        return Some(dir.to_path_buf());
    }

    let hidden = dir.join(".ticket");
    if hidden.join("tickets.db").is_file() || has_ticket_manifest(&hidden) {
        return Some(hidden);
    }

    None
}

fn has_ticket_manifest(store_root: &Path) -> bool {
    let tickets_dir = store_root.join("tickets");
    let Ok(entries) = std::fs::read_dir(tickets_dir) else {
        return false;
    };

    entries
        .flatten()
        .any(|entry| entry.path().join("ticket.toml").is_file())
}

fn ancestor_label(depth: usize) -> String {
    std::iter::repeat("..")
        .take(depth)
        .collect::<Vec<_>>()
        .join("/")
}

fn short_workspace_hash(path: &Path) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    let normalized = path.to_string_lossy().replace('\\', "/");
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:08x}", (hash & 0xffff_ffff) as u32)
}

#[cfg(test)]
#[path = "registry/workspace_resolution_tests.rs"]
mod workspace_resolution_tests;
