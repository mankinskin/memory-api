use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

use chrono::Utc;
use fs2::FileExt;
use memory_api::workspace::{
    WorkspacePathError,
    canonicalize_workspace_root_strict,
    normalize_slashes,
};
use serde::{Deserialize, Serialize};
use session_api::{
    SessionStoreConfig,
    SessionWorktreeStatus,
};
use thiserror::Error;

const ROUTING_DIR: &str = ".session-routing";
const INDEX_FILE: &str = "worktree-index.json";
const LOCK_FILE: &str = "worktree-index.lock";
const REGISTRY_SCHEMA_VERSION: u32 = 1;

/// Canonical repository root, known independently of any process CWD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRoot(PathBuf);

impl RepositoryRoot {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, ResolutionError> {
        Ok(Self(canonicalize(path.as_ref())?))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// The checkout class owning a resolved target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutScope {
    MainCheckout { checkout_root: PathBuf },
    Worktree { worktree_root: PathBuf, branch: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspace {
    repository: RepositoryRoot,
    checkout: CheckoutScope,
    target_root: PathBuf,
    relative_path: PathBuf,
}

impl ResolvedWorkspace {
    pub fn repository_root(&self) -> &Path {
        self.repository.as_path()
    }

    pub fn checkout(&self) -> &CheckoutScope {
        &self.checkout
    }

    pub fn target_root(&self) -> &Path {
        &self.target_root
    }

    pub fn is_worktree(&self) -> bool {
        matches!(self.checkout, CheckoutScope::Worktree { .. })
    }

    pub fn is_main_checkout(&self) -> bool {
        matches!(self.checkout, CheckoutScope::MainCheckout { .. })
    }

    /// Refuses mutations targeted at the repository's main checkout.
    pub fn require_mutation_target(&self) -> Result<(), ResolutionError> {
        if self.is_main_checkout() {
            return Err(ResolutionError::MainCheckoutMutationBlocked);
        }
        Ok(())
    }

    /// Returns `<target_root>/<store_dir>` after validating `store_dir`.
    pub fn store_root(&self, store_dir: &str) -> Result<PathBuf, ResolutionError> {
        validate_store_dir(store_dir)?;
        Ok(self.target_root.join(store_dir))
    }

    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub worktree_path: PathBuf,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegistryFile {
    schema_version: u32,
    entries: BTreeMap<String, RegistryEntry>,
}

impl Default for RegistryFile {
    fn default() -> Self {
        Self {
            schema_version: REGISTRY_SCHEMA_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionWorktreeRegistry {
    main_checkout: RepositoryRoot,
}

impl SessionWorktreeRegistry {
    pub fn new(main_checkout: RepositoryRoot) -> Self {
        Self { main_checkout }
    }

    pub fn index_path(&self) -> PathBuf {
        self.routing_dir().join(INDEX_FILE)
    }

    pub fn lookup(&self, session_id: &str) -> Result<RegistryEntry, ResolutionError> {
        validate_session_id(session_id)?;
        let index = self.read_index()?;
        index.entries.get(session_id).cloned().ok_or_else(|| {
            ResolutionError::RegistryEntryMissing {
                session_id: session_id.to_string(),
            }
        })
    }

    pub fn upsert(
        &self,
        session_id: &str,
        worktree_path: &Path,
    ) -> Result<(), ResolutionError> {
        validate_session_id(session_id)?;
        let canonical_worktree = canonicalize(worktree_path)?;
        let routing_dir = self.routing_dir();
        fs::create_dir_all(&routing_dir).map_err(|source| ResolutionError::Io {
            path: routing_dir.clone(),
            source,
        })?;
        let lock_path = routing_dir.join(LOCK_FILE);
        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| ResolutionError::Io {
                path: lock_path,
                source,
            })?;
        lock_file.lock_exclusive().map_err(|source| ResolutionError::Io {
            path: self.routing_dir().join(LOCK_FILE),
            source,
        })?;

        let mut index = self.read_index_or_default()?;
        index.entries.insert(
            session_id.to_string(),
            RegistryEntry {
                worktree_path: canonical_worktree,
                updated_at: Utc::now().to_rfc3339(),
            },
        );
        self.write_index(&index)
    }

    pub fn remove(&self, session_id: &str) -> Result<(), ResolutionError> {
        validate_session_id(session_id)?;
        let routing_dir = self.routing_dir();
        fs::create_dir_all(&routing_dir).map_err(|source| ResolutionError::Io {
            path: routing_dir.clone(),
            source,
        })?;
        let lock_path = routing_dir.join(LOCK_FILE);
        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| ResolutionError::Io {
                path: lock_path,
                source,
            })?;
        lock_file.lock_exclusive().map_err(|source| ResolutionError::Io {
            path: self.routing_dir().join(LOCK_FILE),
            source,
        })?;

        let mut index = self.read_index_or_default()?;
        index.entries.remove(session_id);
        self.write_index(&index)
    }

    fn routing_dir(&self) -> PathBuf {
        self.main_checkout.as_path().join(ROUTING_DIR)
    }

    fn read_index(&self) -> Result<RegistryFile, ResolutionError> {
        let index_path = self.index_path();
        let contents = fs::read_to_string(&index_path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ResolutionError::RegistryMissing {
                    path: index_path.clone(),
                }
            } else {
                ResolutionError::Io {
                    path: index_path.clone(),
                    source,
                }
            }
        })?;
        parse_index(&index_path, &contents)
    }

    fn read_index_or_default(&self) -> Result<RegistryFile, ResolutionError> {
        match self.read_index() {
            Ok(index) => Ok(index),
            Err(ResolutionError::RegistryMissing { .. }) => Ok(RegistryFile::default()),
            Err(error) => Err(error),
        }
    }

    fn write_index(&self, index: &RegistryFile) -> Result<(), ResolutionError> {
        let routing_dir = self.routing_dir();
        let index_path = self.index_path();
        let bytes = serde_json::to_vec_pretty(index)
            .map_err(|source| ResolutionError::RegistryMalformed {
                path: index_path.clone(),
                detail: source.to_string(),
            })?;
        let mut temp = tempfile::NamedTempFile::new_in(&routing_dir).map_err(|source| {
            ResolutionError::Io {
                path: routing_dir.clone(),
                source,
            }
        })?;
        temp.write_all(&bytes).map_err(|source| ResolutionError::Io {
            path: temp.path().to_path_buf(),
            source,
        })?;
        temp.as_file().sync_all().map_err(|source| ResolutionError::Io {
            path: temp.path().to_path_buf(),
            source,
        })?;
        fs::rename(temp.path(), &index_path).map_err(|source| ResolutionError::Io {
            path: index_path.clone(),
            source,
        })?;
        sync_directory(&routing_dir)?;
        Ok(())
    }
}

pub struct ResolveRequest<'a> {
    pub session_id: &'a str,
    /// Optional path relative to the worktree. It is never a selector.
    pub relative_workspace: Option<&'a Path>,
    /// Entity-store directory for the calling server, for example `.ticket`.
    pub store_dir: &'a str,
}

#[derive(Debug, Clone)]
pub struct ResolverConfig {
    pub main_checkout: PathBuf,
    pub workspace_slug: String,
}

pub struct SessionWorkspaceResolver {
    config: ResolverConfig,
    registry: SessionWorktreeRegistry,
}

impl SessionWorkspaceResolver {
    pub fn new(config: ResolverConfig) -> Result<Self, ResolutionError> {
        if config.workspace_slug.trim().is_empty() {
            return Err(ResolutionError::InvalidConfiguration(
                "workspace_slug must not be empty".to_string(),
            ));
        }
        let main_checkout = RepositoryRoot::new(&config.main_checkout)?;
        Ok(Self {
            config,
            registry: SessionWorktreeRegistry::new(main_checkout),
        })
    }

    /// Resolves the active worktree from current registry and session-store data.
    pub fn resolve(
        &self,
        request: ResolveRequest<'_>,
    ) -> Result<ResolvedWorkspace, ResolutionError> {
        validate_session_id(request.session_id)?;
        validate_store_dir(request.store_dir)?;
        let registry_entry = self.registry.lookup(request.session_id)?;
        let worktree_root = canonicalize(&registry_entry.worktree_path).map_err(|error| {
            match error {
                ResolutionError::InvalidConfiguration(_) => ResolutionError::RegistryWorktreeMissing {
                    path: registry_entry.worktree_path.clone(),
                },
                other => other,
            }
        })?;
        let repository = self.registry.main_checkout.clone();
        if !worktree_root.starts_with(repository.as_path()) {
            return Err(ResolutionError::RegistryWorktreeOutsideRepository {
                path: worktree_root,
                repository: repository.as_path().to_path_buf(),
            });
        }

        let store = SessionStoreConfig::new(
            worktree_root.join(".session"),
            self.config.workspace_slug.clone(),
        );
        let receipt = store.lookup_worktree(request.session_id)?;
        if canonicalize(&receipt.worktree_path)? != worktree_root {
            return Err(ResolutionError::RegistrySessionMismatch {
                session_id: request.session_id.to_string(),
            });
        }
        if receipt.status != SessionWorktreeStatus::Active {
            return Err(ResolutionError::InactiveSessionWorktree {
                session_id: request.session_id.to_string(),
                status: receipt.status,
            });
        }

        let relative_path = resolve_relative_path(&worktree_root, request.relative_workspace)?;
        Ok(ResolvedWorkspace {
            repository,
            checkout: CheckoutScope::Worktree {
                worktree_root: worktree_root.clone(),
                branch: receipt.branch,
            },
            target_root: worktree_root.join(&relative_path),
            relative_path,
        })
    }

    /// Enumerates store candidates for diagnostics without selecting a default.
    pub fn refused_candidates(&self, store_dir: &str) -> Result<Vec<PathBuf>, ResolutionError> {
        validate_store_dir(store_dir)?;
        let mut candidates = vec![self.registry.main_checkout.as_path().join(store_dir)];
        let worktrees_dir = self.registry.main_checkout.as_path().join(".worktrees");
        if let Ok(entries) = fs::read_dir(worktrees_dir) {
            for entry in entries {
                let entry = entry.map_err(|source| ResolutionError::Io {
                    path: self.registry.main_checkout.as_path().join(".worktrees"),
                    source,
                })?;
                if entry.path().is_dir() {
                    candidates.push(entry.path().join(store_dir));
                }
            }
        }
        candidates.sort();
        Ok(candidates)
    }
}

#[derive(Debug, Error)]
pub enum ResolutionError {
    #[error("invalid resolver configuration: {0}")]
    InvalidConfiguration(String),
    #[error("session id is required")]
    MissingSessionId,
    #[error("routing registry is missing: {}", normalize_slashes(path))]
    RegistryMissing { path: PathBuf },
    #[error("routing registry is malformed at {}: {detail}", normalize_slashes(path))]
    RegistryMalformed { path: PathBuf, detail: String },
    #[error("routing registry has no entry for session '{session_id}'")]
    RegistryEntryMissing { session_id: String },
    #[error("routing registry worktree is missing: {}", normalize_slashes(path))]
    RegistryWorktreeMissing { path: PathBuf },
    #[error("routing registry worktree {} is outside repository {}", normalize_slashes(path), normalize_slashes(repository))]
    RegistryWorktreeOutsideRepository { path: PathBuf, repository: PathBuf },
    #[error("routing registry does not match the session assignment for '{session_id}'")]
    RegistrySessionMismatch { session_id: String },
    #[error("session '{session_id}' has inactive worktree assignment: {status:?}")]
    InactiveSessionWorktree { session_id: String, status: SessionWorktreeStatus },
    #[error("relative workspace path must not be absolute: {}", normalize_slashes(path))]
    AbsoluteRelativeWorkspace { path: PathBuf },
    #[error("relative workspace path escapes the worktree: {}", normalize_slashes(path))]
    RelativeWorkspaceEscapesWorktree { path: PathBuf },
    #[error("main checkout mutations are blocked")]
    MainCheckoutMutationBlocked,
    #[error("workspace selector 'default' for session '{session_id}' is unanchored; refused to select a store from candidates: {}", candidates.iter().map(|path| normalize_slashes(path)).collect::<Vec<_>>().join(", "))]
    UnanchoredDefault { session_id: String, candidates: Vec<PathBuf> },
    #[error("session store lookup failed: {0}")]
    SessionLookup(#[from] session_api::SessionError),
    #[error("I/O failed for {}: {source}", normalize_slashes(path))]
    Io { path: PathBuf, source: std::io::Error },
}

fn canonicalize(path: &Path) -> Result<PathBuf, ResolutionError> {
    canonicalize_workspace_root_strict(path).map_err(|error| match error {
        WorkspacePathError::CanonicalizeFailed { input, .. } => {
            ResolutionError::InvalidConfiguration(format!("unable to canonicalize '{input}'"))
        }
        other => ResolutionError::InvalidConfiguration(other.to_string()),
    })
}

fn parse_index(path: &Path, contents: &str) -> Result<RegistryFile, ResolutionError> {
    let index: RegistryFile = serde_json::from_str(contents).map_err(|source| {
        ResolutionError::RegistryMalformed {
            path: path.to_path_buf(),
            detail: source.to_string(),
        }
    })?;
    if index.schema_version != REGISTRY_SCHEMA_VERSION {
        return Err(ResolutionError::RegistryMalformed {
            path: path.to_path_buf(),
            detail: format!("unsupported schema version {}", index.schema_version),
        });
    }
    Ok(index)
}

fn validate_session_id(session_id: &str) -> Result<(), ResolutionError> {
    if session_id.trim().is_empty() {
        return Err(ResolutionError::MissingSessionId);
    }
    Ok(())
}

fn validate_store_dir(store_dir: &str) -> Result<(), ResolutionError> {
    let path = Path::new(store_dir);
    if store_dir.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return Err(ResolutionError::InvalidConfiguration(format!(
            "invalid store directory '{store_dir}'",
        )));
    }
    Ok(())
}

fn resolve_relative_path(
    worktree_root: &Path,
    relative_workspace: Option<&Path>,
) -> Result<PathBuf, ResolutionError> {
    let Some(relative_workspace) = relative_workspace else {
        return Ok(PathBuf::new());
    };
    if relative_workspace.is_absolute() {
        return Err(ResolutionError::AbsoluteRelativeWorkspace {
            path: relative_workspace.to_path_buf(),
        });
    }
    if relative_workspace.components().any(|component| {
        matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
    }) {
        return Err(ResolutionError::RelativeWorkspaceEscapesWorktree {
            path: relative_workspace.to_path_buf(),
        });
    }
    let target = worktree_root.join(relative_workspace);
    let canonical_target = canonicalize(&target).map_err(|_| {
        ResolutionError::RelativeWorkspaceEscapesWorktree {
            path: relative_workspace.to_path_buf(),
        }
    })?;
    if !canonical_target.starts_with(worktree_root) {
        return Err(ResolutionError::RelativeWorkspaceEscapesWorktree {
            path: relative_workspace.to_path_buf(),
        });
    }
    canonical_target.strip_prefix(worktree_root).map(PathBuf::from).map_err(|_| {
        ResolutionError::RelativeWorkspaceEscapesWorktree {
            path: relative_workspace.to_path_buf(),
        }
    })
}

fn sync_directory(_path: &Path) -> Result<(), ResolutionError> {
    #[cfg(unix)]
    {
        fs::File::open(_path)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| ResolutionError::Io {
                path: _path.to_path_buf(),
                source,
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use session_api::{
        SessionStoreConfig,
        SessionWorktreeCheckInRequest,
        SessionWorktreeStatus,
    };
    use tempfile::TempDir;

    use super::*;

    fn fixture() -> (TempDir, PathBuf, PathBuf, SessionWorkspaceResolver) {
        let temp = TempDir::new().unwrap();
        let repository = temp.path().join("repository");
        let worktree = repository.join(".worktrees").join("feature");
        fs::create_dir_all(&worktree).unwrap();
        let resolver = SessionWorkspaceResolver::new(ResolverConfig {
            main_checkout: repository.clone(),
            workspace_slug: "default".to_string(),
        })
        .unwrap();
        (temp, repository, worktree, resolver)
    }

    fn check_in(worktree: &Path, session_id: &str) {
        SessionStoreConfig::new(worktree.join(".session"), "default")
            .check_in_worktree(SessionWorktreeCheckInRequest {
                session_id: session_id.to_string(),
                owner_id: "agent".to_string(),
                ticket_id: "ticket".to_string(),
                worktree_path: worktree.to_path_buf(),
                branch: "agent/session".to_string(),
                predecessor_session_id: None,
            })
            .unwrap();
    }

    fn register(resolver: &SessionWorkspaceResolver, session_id: &str, worktree: &Path) {
        resolver.registry.upsert(session_id, worktree).unwrap();
    }

    #[test]
    fn resolves_active_assignment_to_worktree_scope() {
        let (_temp, _repository, worktree, resolver) = fixture();
        fs::create_dir_all(worktree.join("nested")).unwrap();
        check_in(&worktree, "session-a");
        register(&resolver, "session-a", &worktree);

        let resolved = resolver.resolve(ResolveRequest {
            session_id: "session-a",
            relative_workspace: Some(Path::new("nested")),
            store_dir: ".ticket",
        }).unwrap();

        assert!(matches!(resolved.checkout(), CheckoutScope::Worktree { .. }));
        assert_eq!(resolved.target_root(), worktree.join("nested"));
    }

    #[test]
    fn superseded_assignment_is_inactive() {
        let (_temp, _repository, worktree, resolver) = fixture();
        check_in(&worktree, "session-a");
        register(&resolver, "session-a", &worktree);
        set_status(&worktree, "session-a", SessionWorktreeStatus::Superseded);

        assert!(matches!(resolve_root(&resolver, "session-a"), Err(ResolutionError::InactiveSessionWorktree { .. })));
    }

    #[test]
    fn invalidated_assignment_is_inactive() {
        let (_temp, _repository, worktree, resolver) = fixture();
        check_in(&worktree, "session-a");
        register(&resolver, "session-a", &worktree);
        set_status(&worktree, "session-a", SessionWorktreeStatus::Invalidated);

        assert!(matches!(resolve_root(&resolver, "session-a"), Err(ResolutionError::InactiveSessionWorktree { .. })));
    }

    #[test]
    fn registry_error_variants_are_distinct() {
        let (_temp, _repository, worktree, resolver) = fixture();
        assert!(matches!(resolver.registry.lookup("session-a"), Err(ResolutionError::RegistryMissing { .. })));
        register(&resolver, "session-b", &worktree);
        assert!(matches!(resolver.registry.lookup("session-a"), Err(ResolutionError::RegistryEntryMissing { .. })));
        fs::write(resolver.registry.index_path(), "not json").unwrap();
        assert!(matches!(resolver.registry.lookup("session-b"), Err(ResolutionError::RegistryMalformed { .. })));
    }

    #[test]
    fn deleted_registry_worktree_is_rejected() {
        let (_temp, _repository, worktree, resolver) = fixture();
        register(&resolver, "session-a", &worktree);
        fs::remove_dir_all(worktree).unwrap();

        assert!(matches!(resolve_root(&resolver, "session-a"), Err(ResolutionError::RegistryWorktreeMissing { .. })));
    }

    #[test]
    fn registry_worktree_outside_repository_is_rejected() {
        let (temp, _repository, _worktree, resolver) = fixture();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        register(&resolver, "session-a", &outside);

        assert!(matches!(resolve_root(&resolver, "session-a"), Err(ResolutionError::RegistryWorktreeOutsideRepository { .. })));
    }

    #[test]
    fn relative_workspace_rejects_escape_and_accepts_nested_path() {
        let (_temp, _repository, worktree, resolver) = fixture();
        fs::create_dir_all(worktree.join("nested")).unwrap();
        check_in(&worktree, "session-a");
        register(&resolver, "session-a", &worktree);

        let escaped = resolver.resolve(ResolveRequest {
            session_id: "session-a",
            relative_workspace: Some(Path::new("nested/../../outside")),
            store_dir: ".ticket",
        });
        let nested = resolver.resolve(ResolveRequest {
            session_id: "session-a",
            relative_workspace: Some(Path::new("nested")),
            store_dir: ".ticket",
        });

        assert!(matches!(escaped, Err(ResolutionError::RelativeWorkspaceEscapesWorktree { .. })));
        assert_eq!(nested.unwrap().target_root(), worktree.join("nested"));
    }

    #[test]
    fn mutation_gate_distinguishes_checkout_scopes() {
        let (_temp, repository, worktree, resolver) = fixture();
        let main = ResolvedWorkspace {
            repository: RepositoryRoot::new(&repository).unwrap(),
            checkout: CheckoutScope::MainCheckout { checkout_root: repository.clone() },
            target_root: repository,
            relative_path: PathBuf::new(),
        };
        let worktree = ResolvedWorkspace {
            repository: RepositoryRoot::new(main.repository_root()).unwrap(),
            checkout: CheckoutScope::Worktree { worktree_root: worktree.clone(), branch: "agent/session".to_string() },
            target_root: worktree,
            relative_path: PathBuf::new(),
        };

        assert!(matches!(main.require_mutation_target(), Err(ResolutionError::MainCheckoutMutationBlocked)));
        assert!(worktree.require_mutation_target().is_ok());
        drop(resolver);
    }

    #[test]
    fn upsert_round_trips_without_clobbering_other_entries() {
        let (_temp, repository, worktree, resolver) = fixture();
        let second = repository.join(".worktrees").join("second");
        fs::create_dir_all(&second).unwrap();
        register(&resolver, "session-a", &worktree);
        register(&resolver, "session-b", &second);

        assert_eq!(resolver.registry.lookup("session-a").unwrap().worktree_path, canonicalize(&worktree).unwrap());
        assert_eq!(resolver.registry.lookup("session-b").unwrap().worktree_path, canonicalize(&second).unwrap());
    }

    #[test]
    fn unanchored_default_message_names_candidates() {
        let (_temp, repository, worktree, resolver) = fixture();
        fs::create_dir_all(worktree.join(".ticket")).unwrap();
        let candidates = resolver.refused_candidates(".ticket").unwrap();
        let message = ResolutionError::UnanchoredDefault {
            session_id: "abc".to_string(),
            candidates,
        }.to_string();

        assert!(message.contains(&normalize_slashes(&repository.join(".ticket"))));
        assert!(message.contains(&normalize_slashes(&worktree.join(".ticket"))));
    }

    fn resolve_root(
        resolver: &SessionWorkspaceResolver,
        session_id: &str,
    ) -> Result<ResolvedWorkspace, ResolutionError> {
        resolver.resolve(ResolveRequest {
            session_id,
            relative_workspace: None,
            store_dir: ".ticket",
        })
    }

    fn set_status(worktree: &Path, session_id: &str, status: SessionWorktreeStatus) {
        let config = SessionStoreConfig::new(worktree.join(".session"), "default");
        let mut record = config.read_session(session_id).unwrap();
        record.metadata.worktree.as_mut().unwrap().status = status;
        let path = worktree.join(".session").join("sessions").join(session_id).join("session.json");
        fs::write(path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
    }
}
