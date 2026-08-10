use std::{
    collections::HashMap,
    fs,
    path::{
        Component,
        Path,
        PathBuf,
    },
    sync::Mutex,
};

use memory_api::workspace::{
    WorkspacePathError,
    canonicalize_workspace_root_strict,
    normalize_slashes,
    working_dir,
};
use session_api::{
    SessionStoreConfig,
    SessionWorktreeStatus,
};
use thiserror::Error;

/// Canonical repository root, discovered from the process working directory.
///
/// The MCP servers are always launched at the checkout they serve, so the
/// working directory is the anchor and the `.session` store beneath it is the
/// worktree registry. There is no separate index file and no required
/// environment variable.
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
    MainCheckout {
        checkout_root: PathBuf,
    },
    Worktree {
        worktree_root: PathBuf,
        branch: String,
    },
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
    pub fn store_root(
        &self,
        store_dir: &str,
    ) -> Result<PathBuf, ResolutionError> {
        validate_store_dir(store_dir)?;
        let store_root = self.target_root.join(store_dir);
        let canonical_target = canonicalize(&self.target_root)?;
        let canonical_ancestor = canonicalize_existing_ancestor(&store_root)?;
        if !canonical_ancestor.starts_with(&canonical_target) {
            return Err(ResolutionError::StoreDirectoryEscapesTarget {
                store_dir: store_dir.to_string(),
            });
        }
        Ok(store_root)
    }

    /// Returns a validated store root only when the resolved target permits mutation.
    pub fn mutation_store_root(
        &self,
        store_dir: &str,
    ) -> Result<PathBuf, ResolutionError> {
        self.require_mutation_target()?;
        self.store_root(store_dir)
    }

    pub fn relative_path(&self) -> &Path {
        &self.relative_path
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

impl ResolverConfig {
    /// Anchors resolution on the process working directory.
    ///
    /// MCP servers are launched at the checkout they serve — including when
    /// that checkout is a submodule working directory rather than a
    /// superproject root — so the working directory is the anchor. A
    /// `git --git-common-dir` walk would escape to the superproject, and an
    /// environment variable would only restate what the working directory
    /// already says.
    pub fn from_working_dir(
        workspace_slug: impl Into<String>
    ) -> Result<Self, ResolutionError> {
        let main_checkout = working_dir().ok_or_else(|| {
            ResolutionError::InvalidConfiguration(
                "unable to determine the process working directory".to_string(),
            )
        })?;
        Ok(Self {
            main_checkout,
            workspace_slug: workspace_slug.into(),
        })
    }
}

pub struct SessionWorkspaceResolver {
    config: ResolverConfig,
    main_checkout: RepositoryRoot,
    /// Memoizes successful discoveries for the process lifetime.
    ///
    /// Only successes are cached. A miss is not a stable fact: the
    /// `UserPromptSubmit` hook may create the worktree moments after a failed
    /// lookup, and a cached miss would outlive the condition that produced it.
    discovery_cache: Mutex<HashMap<String, DiscoveredWorktree>>,
}

/// A worktree located by filesystem discovery rather than by an assignment
/// recorded in the session store.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredWorktree {
    root: PathBuf,
    branch: String,
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
            main_checkout,
            discovery_cache: Mutex::new(HashMap::new()),
        })
    }

    /// The `.session` store beneath the anchor, which is the sole registry of
    /// session-to-worktree assignments.
    fn session_store(&self) -> SessionStoreConfig {
        SessionStoreConfig::new(
            self.main_checkout.as_path().join(".session"),
            self.config.workspace_slug.clone(),
        )
    }

    /// Resolves the active worktree from current registry and session-store data.
    pub fn resolve(
        &self,
        request: ResolveRequest<'_>,
    ) -> Result<ResolvedWorkspace, ResolutionError> {
        validate_session_id(request.session_id)?;
        validate_store_dir(request.store_dir)?;
        let repository = self.main_checkout.clone();
        let assignment = match self
            .session_store()
            .lookup_worktree(request.session_id)
        {
            Ok(receipt) => {
                Assignment {
                    worktree_path: receipt.worktree_path,
                    branch: receipt.branch,
                    status: receipt.status,
                }
            },
            Err(
                session_api::SessionError::MissingWorktreeAssignment {
                    ..
                }
                | session_api::SessionError::NotFound { .. },
            ) => {
                // The configured checkout contains the invocation. Do not
                // enumerate sibling worktrees to infer a different store.
                Assignment {
                    worktree_path: self.main_checkout.as_path().to_path_buf(),
                    branch: String::new(),
                    status: SessionWorktreeStatus::Active,
                }
            },
            Err(error) => return Err(ResolutionError::SessionLookup(error)),
        };
        let worktree_root = canonicalize(&assignment.worktree_path).map_err(
            |error| match error {
                ResolutionError::InvalidConfiguration(_) =>
                    ResolutionError::SessionWorktreeMissing {
                        path: assignment.worktree_path.clone(),
                    },
                other => other,
            },
        )?;
        if !worktree_root.starts_with(repository.as_path()) {
            return Err(ResolutionError::SessionWorktreeOutsideRepository {
                path: worktree_root,
                repository: repository.as_path().to_path_buf(),
            });
        }
        if !is_git_checkout(&worktree_root)? {
            return Err(ResolutionError::SessionWorktreeNotGitCheckout {
                path: worktree_root,
            });
        }
        if assignment.status != SessionWorktreeStatus::Active {
            return Err(ResolutionError::InactiveSessionWorktree {
                session_id: request.session_id.to_string(),
                status: assignment.status,
            });
        }

        let relative_path =
            resolve_relative_path(&worktree_root, request.relative_workspace)?;
        let invocation_is_linked_worktree = repository
            .as_path()
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == ".worktrees");
        let checkout = if worktree_root == repository.as_path()
            && !invocation_is_linked_worktree
        {
            CheckoutScope::MainCheckout {
                checkout_root: worktree_root.clone(),
            }
        } else {
            CheckoutScope::Worktree {
                worktree_root: worktree_root.clone(),
                branch: assignment.branch,
            }
        };
        Ok(ResolvedWorkspace {
            repository,
            checkout,
            target_root: worktree_root.join(&relative_path),
            relative_path,
        })
    }

    /// Locates a session's worktree on disk when the session store holds no
    /// assignment.
    ///
    /// Two strategies, in order:
    ///
    /// 1. A glob over `.worktrees/<short-id>-*`, where `<short-id>` is the
    ///    leading eight characters of the session id. This is the naming
    ///    convention every worktree bootstrapped by the repository tooling
    ///    follows, so it resolves without reading any file.
    /// 2. Failing that, a scan of `.worktrees/*/` for a worktree whose own
    ///    `.session` store contains a record for this session. This catches
    ///    worktrees named by ticket id rather than session id.
    ///
    /// Returns `Ok(None)` when neither strategy finds a candidate. Two or more
    /// glob candidates is an error, never an arbitrary choice.
    fn discover_worktree(
        &self,
        session_id: &str,
    ) -> Result<Option<DiscoveredWorktree>, ResolutionError> {
        if let Some(cached) = self
            .discovery_cache
            .lock()
            .expect("discovery cache mutex poisoned")
            .get(session_id)
        {
            return Ok(Some(cached.clone()));
        }

        let worktrees_dir = self.main_checkout.as_path().join(".worktrees");
        let Some(entries) = read_worktree_entries(&worktrees_dir)? else {
            return Ok(None);
        };

        let short_id = session_short_id(session_id);
        let prefix = format!("{short_id}-");
        let mut matches: Vec<PathBuf> = entries
            .iter()
            .filter(|entry| {
                entry
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix))
            })
            .cloned()
            .collect();
        matches.sort();

        let root = match matches.len() {
            1 => matches.remove(0),
            0 => {
                let Some(scanned) = scan_for_session_record(&entries, session_id)?
                else {
                    return Ok(None);
                };
                scanned
            },
            _ =>
                return Err(ResolutionError::AmbiguousSessionWorktree {
                    session_id: session_id.to_string(),
                    candidates: matches,
                }),
        };

        if !is_git_checkout(&root)? {
            return Err(ResolutionError::SessionWorktreeNotGitCheckout {
                path: root,
            });
        }

        let discovered = DiscoveredWorktree {
            branch: read_checked_out_branch(&root)?,
            root,
        };
        self.discovery_cache
            .lock()
            .expect("discovery cache mutex poisoned")
            .insert(session_id.to_string(), discovered.clone());
        Ok(Some(discovered))
    }

    /// Returns only the invocation checkout's store for diagnostics.
    pub fn refused_candidates(
        &self,
        store_dir: &str,
    ) -> Result<Vec<PathBuf>, ResolutionError> {
        validate_store_dir(store_dir)?;
        Ok(vec![self.main_checkout.as_path().join(store_dir)])
    }
}

#[derive(Debug, Error)]
pub enum ResolutionError {
    #[error("invalid resolver configuration: {0}")]
    InvalidConfiguration(String),
    #[error("session id is required")]
    MissingSessionId,
    #[error(
        "session '{session_id}' has no worktree assignment in the session store"
    )]
    MissingSessionWorktree { session_id: String },
    #[error(
        "session '{session_id}' matches {} worktrees; refusing to choose: {}",
        candidates.len(),
        candidates.iter().map(|path| normalize_slashes(path)).collect::<Vec<_>>().join(", ")
    )]
    AmbiguousSessionWorktree {
        session_id: String,
        candidates: Vec<PathBuf>,
    },
    #[error(
        "assigned session worktree is missing: {}",
        normalize_slashes(path)
    )]
    SessionWorktreeMissing { path: PathBuf },
    #[error(
        "assigned session worktree {} is outside repository {}",
        normalize_slashes(path),
        normalize_slashes(repository)
    )]
    SessionWorktreeOutsideRepository { path: PathBuf, repository: PathBuf },
    #[error(
        "assigned session worktree is not a git checkout: {}",
        normalize_slashes(path)
    )]
    SessionWorktreeNotGitCheckout { path: PathBuf },
    #[error(
        "session '{session_id}' has inactive worktree assignment: {status:?}"
    )]
    InactiveSessionWorktree {
        session_id: String,
        status: SessionWorktreeStatus,
    },
    #[error(
        "relative workspace path must not be absolute: {}",
        normalize_slashes(path)
    )]
    AbsoluteRelativeWorkspace { path: PathBuf },
    #[error(
        "relative workspace path escapes the worktree: {}",
        normalize_slashes(path)
    )]
    RelativeWorkspaceEscapesWorktree { path: PathBuf },
    #[error("main checkout mutations are blocked; run session_check_in from an assigned worktree path under <repository>/.worktrees/<name> and retry")]
    MainCheckoutMutationBlocked,
    #[error("store directory escapes resolved target: '{store_dir}'")]
    StoreDirectoryEscapesTarget { store_dir: String },
    #[error("workspace selector 'default' for session '{session_id}' is unanchored; refused to select a store from candidates: {}", candidates.iter().map(|path| normalize_slashes(path)).collect::<Vec<_>>().join(", "))]
    UnanchoredDefault {
        session_id: String,
        candidates: Vec<PathBuf>,
    },
    #[error("session store lookup failed: {0}")]
    SessionLookup(#[from] session_api::SessionError),
    #[error("I/O failed for {}: {source}", normalize_slashes(path))]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

fn canonicalize(path: &Path) -> Result<PathBuf, ResolutionError> {
    canonicalize_workspace_root_strict(path).map_err(|error| match error {
        WorkspacePathError::CanonicalizeFailed { input, .. } =>
            ResolutionError::InvalidConfiguration(format!(
                "unable to canonicalize '{input}'"
            )),
        other => ResolutionError::InvalidConfiguration(other.to_string()),
    })
}

fn paths_refer_to_same_directory(left: &Path, right: &Path) -> bool {
    match (canonicalize(left), canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => normalize_slashes(left)
            .eq_ignore_ascii_case(&normalize_slashes(right)),
    }
}

/// A session-to-worktree assignment, however it was obtained: recorded in the
/// session store, or discovered on disk.
struct Assignment {
    worktree_path: PathBuf,
    branch: String,
    status: SessionWorktreeStatus,
}

/// The leading eight characters of a session id, which is the prefix the
/// worktree naming convention uses.
fn session_short_id(session_id: &str) -> &str {
    let end = session_id
        .char_indices()
        .nth(8)
        .map_or(session_id.len(), |(index, _)| index);
    &session_id[..end]
}

/// Lists the directories directly under `.worktrees`, or `None` when that
/// directory does not exist. A missing `.worktrees` is ordinary, not an error.
fn read_worktree_entries(
    worktrees_dir: &Path
) -> Result<Option<Vec<PathBuf>>, ResolutionError> {
    let entries = match fs::read_dir(worktrees_dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound =>
            return Ok(None),
        Err(source) =>
            return Err(ResolutionError::Io {
                path: worktrees_dir.to_path_buf(),
                source,
            }),
    };
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| ResolutionError::Io {
            path: worktrees_dir.to_path_buf(),
            source,
        })?;
        if entry.path().is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(Some(directories))
}

/// Finds the worktree whose own `.session` store holds a record for this
/// session. Used when the name-based glob finds nothing.
fn scan_for_session_record(
    entries: &[PathBuf],
    session_id: &str,
) -> Result<Option<PathBuf>, ResolutionError> {
    for entry in entries {
        let record = entry
            .join(".session")
            .join("sessions")
            .join(session_id)
            .join("session.json");
        if record.is_file() {
            return Ok(Some(entry.clone()));
        }
    }
    Ok(None)
}

/// Reads the branch checked out in a linked worktree.
///
/// A linked worktree's `.git` is a file pointing at its private git directory
/// under the main checkout's `.git/worktrees/<name>`; that directory holds the
/// worktree's own `HEAD`. A detached head has no branch name, which is
/// reported as an empty string rather than an error, matching how the session
/// store records an unnamed branch.
fn read_checked_out_branch(
    worktree_root: &Path
) -> Result<String, ResolutionError> {
    let git_entry = worktree_root.join(".git");
    let metadata =
        fs::metadata(&git_entry).map_err(|source| ResolutionError::Io {
            path: git_entry.clone(),
            source,
        })?;

    let git_dir = if metadata.is_dir() {
        git_entry
    } else {
        let contents = fs::read_to_string(&git_entry).map_err(|source| {
            ResolutionError::Io {
                path: git_entry.clone(),
                source,
            }
        })?;
        let pointer = contents
            .lines()
            .find_map(|line| line.trim().strip_prefix("gitdir:"))
            .map(str::trim)
            .ok_or_else(|| ResolutionError::SessionWorktreeNotGitCheckout {
                path: worktree_root.to_path_buf(),
            })?;
        let pointer = Path::new(pointer);
        if pointer.is_absolute() {
            pointer.to_path_buf()
        } else {
            worktree_root.join(pointer)
        }
    };

    let head_path = git_dir.join("HEAD");
    let head = fs::read_to_string(&head_path).map_err(|source| {
        ResolutionError::Io {
            path: head_path,
            source,
        }
    })?;
    Ok(head
        .trim()
        .strip_prefix("ref: refs/heads/")
        .unwrap_or_default()
        .to_string())
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
            matches!(
                component,
                Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(ResolutionError::InvalidConfiguration(format!(
            "invalid store directory '{store_dir}'",
        )));
    }
    Ok(())
}

fn canonicalize_existing_ancestor(
    path: &Path
) -> Result<PathBuf, ResolutionError> {
    let mut ancestor = path;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or_else(|| {
            ResolutionError::InvalidConfiguration(format!(
                "unable to find existing ancestor for '{}'",
                normalize_slashes(path)
            ))
        })?;
    }
    canonicalize(ancestor)
}

fn is_git_checkout(root: &Path) -> Result<bool, ResolutionError> {
    let git_entry = root.join(".git");
    match fs::metadata(&git_entry) {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(metadata) if metadata.is_file() => fs::read_to_string(&git_entry)
            .map(|contents| {
                contents
                    .lines()
                    .any(|line| line.trim_start().starts_with("gitdir:"))
            })
            .map_err(|source| ResolutionError::Io {
                path: git_entry,
                source,
            }),
        Ok(_) | Err(_) => Ok(false),
    }
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
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
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
    canonical_target
        .strip_prefix(worktree_root)
        .map(PathBuf::from)
        .map_err(|_| ResolutionError::RelativeWorkspaceEscapesWorktree {
            path: relative_workspace.to_path_buf(),
        })
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
        fs::create_dir_all(repository.join(".git")).unwrap();
        fs::create_dir_all(worktree.join(".git")).unwrap();
        let resolver = SessionWorkspaceResolver::new(ResolverConfig {
            main_checkout: repository.clone(),
            workspace_slug: "default".to_string(),
        })
        .unwrap();
        (temp, repository, worktree, resolver)
    }

    fn check_in(
        store_root: &Path,
        worktree: &Path,
        session_id: &str,
    ) {
        SessionStoreConfig::new(store_root.join(".session"), "default")
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

    #[test]
    fn resolves_active_assignment_to_worktree_scope() {
        let (_temp, repository, worktree, resolver) = fixture();
        fs::create_dir_all(worktree.join("nested")).unwrap();
        check_in(&repository, &worktree, "session-a");

        let resolved = resolver
            .resolve(ResolveRequest {
                session_id: "session-a",
                relative_workspace: Some(Path::new("nested")),
                store_dir: ".ticket",
            })
            .unwrap();

        assert!(matches!(
            resolved.checkout(),
            CheckoutScope::Worktree { .. }
        ));
        assert_eq!(resolved.target_root(), worktree.join("nested"));
    }

    #[test]
    fn superseded_assignment_is_inactive() {
        let (_temp, repository, worktree, resolver) = fixture();
        check_in(&repository, &worktree, "session-a");
        set_status(
            &repository,
            "session-a",
            SessionWorktreeStatus::Superseded,
        );

        assert!(matches!(
            resolve_root(&resolver, "session-a"),
            Err(ResolutionError::InactiveSessionWorktree { .. })
        ));
    }

    #[test]
    fn invalidated_assignment_is_inactive() {
        let (_temp, repository, worktree, resolver) = fixture();
        check_in(&repository, &worktree, "session-a");
        set_status(
            &repository,
            "session-a",
            SessionWorktreeStatus::Invalidated,
        );

        assert!(matches!(
            resolve_root(&resolver, "session-a"),
            Err(ResolutionError::InactiveSessionWorktree { .. })
        ));
    }

    #[test]
    fn session_without_assignment_uses_invocation_checkout() {
        let (_temp, repository, _worktree, resolver) = fixture();

        assert_eq!(
            resolve_root(&resolver, "session-a").unwrap().target_root(),
            repository
        );
    }

    #[test]
    fn deleted_session_worktree_is_rejected() {
        let (_temp, repository, worktree, resolver) = fixture();
        check_in(&repository, &worktree, "session-a");
        fs::remove_dir_all(worktree).unwrap();

        assert!(matches!(
            resolve_root(&resolver, "session-a"),
            Err(ResolutionError::SessionWorktreeMissing { .. })
        ));
    }

    #[test]
    fn session_worktree_outside_repository_is_rejected() {
        let (temp, repository, _worktree, resolver) = fixture();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        check_in(&repository, &outside, "session-a");

        assert!(matches!(
            resolve_root(&resolver, "session-a"),
            Err(ResolutionError::SessionWorktreeOutsideRepository { .. })
        ));
    }

    #[test]
    fn relative_workspace_rejects_escape_and_accepts_nested_path() {
        let (_temp, repository, worktree, resolver) = fixture();
        fs::create_dir_all(worktree.join("nested")).unwrap();
        check_in(&repository, &worktree, "session-a");

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

        assert!(matches!(
            escaped,
            Err(ResolutionError::RelativeWorkspaceEscapesWorktree { .. })
        ));
        assert_eq!(nested.unwrap().target_root(), worktree.join("nested"));
    }

    #[test]
    fn mutation_gate_distinguishes_checkout_scopes() {
        let (_temp, repository, worktree, resolver) = fixture();
        let main = ResolvedWorkspace {
            repository: RepositoryRoot::new(&repository).unwrap(),
            checkout: CheckoutScope::MainCheckout {
                checkout_root: repository.clone(),
            },
            target_root: repository,
            relative_path: PathBuf::new(),
        };
        let worktree = ResolvedWorkspace {
            repository: RepositoryRoot::new(main.repository_root()).unwrap(),
            checkout: CheckoutScope::Worktree {
                worktree_root: worktree.clone(),
                branch: "agent/session".to_string(),
            },
            target_root: worktree,
            relative_path: PathBuf::new(),
        };

        assert!(matches!(
            main.require_mutation_target(),
            Err(ResolutionError::MainCheckoutMutationBlocked)
        ));
        assert!(worktree.require_mutation_target().is_ok());
        drop(resolver);
    }

    #[test]
    fn store_root_rejects_symlink_escape() {
        let (temp, repository, worktree, _resolver) = fixture();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let link = worktree.join("escape");
        if let Err(error) = create_dir_symlink(&outside, &link) {
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314)
            {
                return;
            }
            panic!("failed to create symlink: {error}");
        }
        let resolved = ResolvedWorkspace {
            repository: RepositoryRoot::new(&repository).unwrap(),
            checkout: CheckoutScope::Worktree {
                worktree_root: worktree.clone(),
                branch: "agent/session".to_string(),
            },
            target_root: worktree,
            relative_path: PathBuf::new(),
        };

        assert!(matches!(
            resolved.store_root("escape/.ticket"),
            Err(ResolutionError::StoreDirectoryEscapesTarget { store_dir }) if store_dir == "escape/.ticket"
        ));
    }

    #[test]
    fn store_root_allows_missing_directory_and_rejects_lexical_escapes() {
        let (_temp, repository, worktree, _resolver) = fixture();
        let resolved = ResolvedWorkspace {
            repository: RepositoryRoot::new(&repository).unwrap(),
            checkout: CheckoutScope::Worktree {
                worktree_root: worktree.clone(),
                branch: "agent/session".to_string(),
            },
            target_root: worktree.clone(),
            relative_path: PathBuf::new(),
        };

        assert_eq!(
            resolved.store_root(".ticket").unwrap(),
            worktree.join(".ticket")
        );
        assert!(matches!(
            resolved.store_root("../.ticket"),
            Err(ResolutionError::InvalidConfiguration(_))
        ));
        assert!(matches!(
            resolved.store_root("/absolute"),
            Err(ResolutionError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn resolve_classifies_main_checkout_and_blocks_mutation() {
        let (_temp, repository, _worktree, resolver) = fixture();
        check_in(&repository, &repository, "session-a");

        let resolved = resolve_root(&resolver, "session-a").unwrap();

        assert!(matches!(
            resolved.checkout(),
            CheckoutScope::MainCheckout { .. }
        ));
        assert!(matches!(
            resolved.require_mutation_target(),
            Err(ResolutionError::MainCheckoutMutationBlocked)
        ));
        assert!(matches!(
            resolved.mutation_store_root(".ticket"),
            Err(ResolutionError::MainCheckoutMutationBlocked)
        ));
    }

    #[test]
    fn resolve_accepts_linked_worktree_git_file() {
        let (_temp, repository, worktree, resolver) = fixture();
        fs::remove_dir(worktree.join(".git")).unwrap();
        fs::write(worktree.join(".git"), "gitdir: /temporary/git/dir\n")
            .unwrap();
        check_in(&repository, &worktree, "session-a");

        let resolved = resolve_root(&resolver, "session-a").unwrap();

        assert!(matches!(
            resolved.checkout(),
            CheckoutScope::Worktree { .. }
        ));
        assert!(resolved.require_mutation_target().is_ok());
    }

    #[test]
    fn resolve_rejects_session_worktree_without_git_entry() {
        let (_temp, repository, worktree, resolver) = fixture();
        fs::remove_dir(worktree.join(".git")).unwrap();
        check_in(&repository, &worktree, "session-a");

        assert!(matches!(
            resolve_root(&resolver, "session-a"),
            Err(ResolutionError::SessionWorktreeNotGitCheckout { .. })
        ));
    }

    #[test]
    fn root_invocation_ignores_sibling_worktree_stores() {
        let (_temp, repository, worktree, resolver) = fixture();
        fs::create_dir_all(repository.join(".ticket")).unwrap();
        fs::create_dir_all(worktree.join(".ticket")).unwrap();
        for number in 0..15 {
            fs::create_dir_all(
                repository
                    .join(".worktrees")
                    .join(format!("sibling-{number}"))
                    .join(".ticket"),
            )
            .unwrap();
        }

        let resolved = resolve_root(&resolver, "unassigned").unwrap();

        assert_eq!(resolved.target_root(), repository);
        assert_eq!(
            resolved.store_root(".ticket").unwrap(),
            repository.join(".ticket")
        );
        assert_eq!(
            resolver.refused_candidates(".ticket").unwrap(),
            vec![repository.join(".ticket")]
        );
    }

    #[test]
    fn worktree_invocation_uses_its_own_store() {
        let (_temp, repository, worktree, _resolver) = fixture();
        fs::create_dir_all(worktree.join(".ticket")).unwrap();
        let resolver = SessionWorkspaceResolver::new(ResolverConfig {
            main_checkout: worktree.clone(),
            workspace_slug: "default".to_string(),
        })
        .unwrap();

        let resolved = resolve_root(&resolver, "unassigned").unwrap();

        assert_eq!(resolved.target_root(), worktree);
        assert_eq!(
            resolved.store_root(".ticket").unwrap(),
            resolved.target_root().join(".ticket")
        );
        assert_eq!(
            resolved.mutation_store_root(".ticket").unwrap(),
            resolved.target_root().join(".ticket")
        );
        assert_ne!(resolved.target_root(), repository);
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

    fn set_status(
        store_root: &Path,
        session_id: &str,
        status: SessionWorktreeStatus,
    ) {
        let config =
            SessionStoreConfig::new(store_root.join(".session"), "default");
        let mut record = config.read_session(session_id).unwrap();
        record.metadata.worktree.as_mut().unwrap().status = status;
        let path = store_root
            .join(".session")
            .join("sessions")
            .join(session_id)
            .join("session.json");
        fs::write(path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
    }

    #[cfg(unix)]
    fn create_dir_symlink(
        target: &Path,
        link: &Path,
    ) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_dir_symlink(
        target: &Path,
        link: &Path,
    ) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(target, link)
    }

    // ---- discovery ----------------------------------------------------
    //
    // Discovery is what runs when the session store holds no assignment,
    // which is the ordinary case for a worktree created outside a check-in.

    const SESSION: &str = "70abae1b-14c4-4033-9265-d37fe08b02b2";

    /// Creates a worktree directory whose `.git` is a real directory, as an
    /// ordinary repository has.
    fn make_worktree(
        repository: &Path,
        name: &str,
        branch: &str,
    ) -> PathBuf {
        let root = repository.join(".worktrees").join(name);
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            root.join(".git").join("HEAD"),
            format!("ref: refs/heads/{branch}\n"),
        )
        .unwrap();
        root
    }

    /// Writes a session record into a worktree's own `.session` store, which
    /// is what the scan fallback looks for.
    fn seed_session_record(
        worktree: &Path,
        session_id: &str,
    ) {
        let dir = worktree.join(".session").join("sessions").join(session_id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("session.json"), "{}").unwrap();
    }

    #[test]
    fn sibling_matching_session_id_does_not_change_root_resolution() {
        let (_temp, repository, _worktree, resolver) = fixture();
        make_worktree(&repository, "70abae1b-some-slug", "agent/some-slug");

        let resolved = resolver
            .resolve(ResolveRequest {
                session_id: SESSION,
                relative_workspace: None,
                store_dir: ".ticket",
            })
            .unwrap();

        assert_eq!(resolved.target_root(), repository);
    }

    #[test]
    fn sibling_for_another_session_does_not_change_root_resolution() {
        let (_temp, repository, _worktree, resolver) = fixture();
        make_worktree(&repository, "deadbeef-other-slug", "agent/other");

        assert_eq!(resolve_root(&resolver, SESSION).unwrap().target_root(), repository);
    }

    #[test]
    fn sibling_session_record_does_not_change_root_resolution() {
        let (_temp, repository, _worktree, resolver) = fixture();
        let worktree =
            make_worktree(&repository, "a1b911ab-by-ticket-id", "agent/ticket");
        seed_session_record(&worktree, SESSION);

        let resolved = resolver
            .resolve(ResolveRequest {
                session_id: SESSION,
                relative_workspace: None,
                store_dir: ".ticket",
            })
            .unwrap();

        assert_eq!(resolved.target_root(), repository);
    }

    #[test]
    fn sibling_prefix_matches_do_not_make_resolution_ambiguous() {
        let (_temp, repository, _worktree, resolver) = fixture();
        make_worktree(&repository, "70abae1b-first", "agent/first");
        make_worktree(&repository, "70abae1b-second", "agent/second");

        assert_eq!(resolve_root(&resolver, SESSION).unwrap().target_root(), repository);
    }

    #[test]
    fn missing_assignment_falls_back_to_invocation_checkout() {
        let (_temp, repository, _worktree, resolver) = fixture();

        assert_eq!(resolve_root(&resolver, SESSION).unwrap().target_root(), repository);
    }

    #[test]
    fn new_sibling_worktrees_do_not_change_root_resolution() {
        let (_temp, repository, _worktree, resolver) = fixture();
        make_worktree(&repository, "70abae1b-some-slug", "agent/some-slug");
        assert_eq!(
            resolve_root(&resolver, SESSION).unwrap().target_root(),
            repository
        );

        make_worktree(&repository, "70abae1b-appeared-later", "agent/later");

        assert_eq!(
            resolve_root(&resolver, SESSION).unwrap().target_root(),
            repository
        );
    }

    #[test]
    fn linked_sibling_worktree_does_not_change_root_resolution() {
        let (_temp, repository, _worktree, resolver) = fixture();
        // A linked worktree's `.git` is a file pointing at its private git
        // directory under the main checkout, which holds its own HEAD.
        let private_git_dir =
            repository.join(".git").join("worktrees").join("linked");
        fs::create_dir_all(&private_git_dir).unwrap();
        fs::write(
            private_git_dir.join("HEAD"),
            "ref: refs/heads/agent/linked\n",
        )
        .unwrap();

        let root = repository.join(".worktrees").join("70abae1b-linked");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join(".git"),
            format!("gitdir: {}\n", private_git_dir.display()),
        )
        .unwrap();

        let resolved = resolver
            .resolve(ResolveRequest {
                session_id: SESSION,
                relative_workspace: None,
                store_dir: ".ticket",
            })
            .unwrap();

        assert_eq!(resolved.target_root(), repository);
        assert!(matches!(resolved.checkout(), CheckoutScope::MainCheckout { .. }));
    }

    #[test]
    fn a_recorded_assignment_wins_over_a_discoverable_worktree() {
        let (_temp, repository, worktree, resolver) = fixture();
        check_in(&repository, &worktree, SESSION);
        make_worktree(&repository, "70abae1b-would-be-discovered", "agent/no");

        assert_eq!(
            resolve_root(&resolver, SESSION).unwrap().target_root(),
            worktree
        );
    }

    #[test]
    fn a_main_pointing_record_uses_the_invocation_checkout() {
        let (_temp, repository, _worktree, resolver) = fixture();
        check_in(&repository, &repository, SESSION);
        make_worktree(&repository, "70abae1b-something", "agent/something");

        assert_eq!(
            resolve_root(&resolver, SESSION).unwrap().target_root(),
            repository
        );
    }

    #[test]
    fn a_main_pointing_record_is_honored_when_nothing_is_discoverable() {
        let (_temp, repository, _worktree, resolver) = fixture();
        check_in(&repository, &repository, SESSION);

        assert_eq!(
            resolve_root(&resolver, SESSION).unwrap().target_root(),
            repository
        );
    }
}
