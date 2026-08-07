use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    time::{
        Duration,
        SystemTime,
    },
};

use serde_json::Value;
use thiserror::Error;
use time::{
    OffsetDateTime,
    format_description::well_known::Rfc3339,
};

use crate::{
    WorktreeGit,
    WorktreeGitError,
    WorktreeRef,
};

const DEFAULT_MAX_WORKTREES: usize = 8;
const DEFAULT_STALE_AFTER: Duration = Duration::from_secs(4 * 60 * 60);

/// Determines whether a live session currently owns a worktree.
pub trait SessionActivity {
    /// True when some live session currently owns this worktree.
    fn is_active(
        &self,
        worktree: &Path,
    ) -> bool;
}

/// Session activity backed by records in a `.session` store.
pub struct SessionStoreActivity {
    session_store: PathBuf,
    stale_after: Duration,
}

impl SessionStoreActivity {
    pub fn new(
        session_store: impl Into<PathBuf>,
        stale_after: Duration,
    ) -> Self {
        Self {
            session_store: session_store.into(),
            stale_after,
        }
    }

    pub fn with_default_staleness(session_store: impl Into<PathBuf>) -> Self {
        Self::new(session_store, DEFAULT_STALE_AFTER)
    }
}

impl SessionActivity for SessionStoreActivity {
    fn is_active(
        &self,
        worktree: &Path,
    ) -> bool {
        let Some(worktree) = normalized_path(worktree) else {
            return false;
        };
        let sessions = self.session_store.join("sessions");
        let Ok(entries) = fs::read_dir(sessions) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let record = entry.path().join("session.json");
            session_record_is_active(&record, &worktree, self.stale_after)
        })
    }
}

/// Test activity implementation that never marks a worktree active.
#[derive(Debug, Default)]
pub struct NeverActive;

impl SessionActivity for NeverActive {
    fn is_active(
        &self,
        _worktree: &Path,
    ) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionPolicy {
    pub max_worktrees: usize,
    pub stale_after: Duration,
    pub base_ref: String,
}

impl Default for ProvisionPolicy {
    fn default() -> Self {
        Self {
            max_worktrees: env_usize("WORKTREE_MAX")
                .unwrap_or(DEFAULT_MAX_WORKTREES),
            stale_after: Duration::from_secs(
                env_u64("WORKTREE_STALE_SECS")
                    .unwrap_or(DEFAULT_STALE_AFTER.as_secs()),
            ),
            base_ref: "main".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionOutcome {
    AlreadyProvisioned(WorktreeRef),
    Reclaimed {
        worktree: WorktreeRef,
        previous_name: String,
    },
    Created(WorktreeRef),
}

#[derive(Debug, Error)]
pub enum ProvisionError {
    #[error(transparent)]
    Git(#[from] WorktreeGitError),
    #[error(
        "worktree cap {max_worktrees} reached with {current_count} registered worktrees; no reclaimable candidate: {reason}"
    )]
    CapReached {
        max_worktrees: usize,
        current_count: usize,
        reason: String,
    },
}

pub fn provision_for_session(
    git: &WorktreeGit,
    activity: &dyn SessionActivity,
    session_id: &str,
    policy: &ProvisionPolicy,
) -> Result<ProvisionOutcome, ProvisionError> {
    let short_id = session_short_id(session_id);
    let name = format!("{short_id}-session");
    let branch = format!("agent/{name}");
    let worktrees = registered_worktrees(git)?;
    let prefix = format!("{short_id}-");

    if let Some(worktree) = worktrees
        .iter()
        .find(|worktree| worktree.name.starts_with(&prefix))
    {
        return Ok(ProvisionOutcome::AlreadyProvisioned(worktree.clone()));
    }

    let mut candidates = Vec::new();
    for worktree in &worktrees {
        if reclaimable(git, activity, worktree)? {
            candidates.push(worktree);
        }
    }
    let candidate = candidates.into_iter().min_by(reclaim_order);

    if let Some(candidate) = candidate {
        let previous_name = candidate.name.clone();
        // TODO(5e6cf4f8): update reclaimed worktrees from main in a later provisioning unit.
        let worktree = git.rename_worktree(&previous_name, &name, &branch)?;
        return Ok(ProvisionOutcome::Reclaimed {
            worktree,
            previous_name,
        });
    }

    if worktrees.len() >= policy.max_worktrees {
        return Err(ProvisionError::CapReached {
            max_worktrees: policy.max_worktrees,
            current_count: worktrees.len(),
            reason: "all registered worktrees are active, dirty, ahead of main, detached, or outside .worktrees".to_string(),
        });
    }

    Ok(ProvisionOutcome::Created(git.create_worktree(
        &name,
        &branch,
        &policy.base_ref,
    )?))
}

fn registered_worktrees(
    git: &WorktreeGit
) -> Result<Vec<WorktreeRef>, WorktreeGitError> {
    let root = git.main_checkout().join(".worktrees");
    Ok(git
        .list_worktrees()?
        .into_iter()
        .filter(|worktree| worktree.path.parent() == Some(root.as_path()))
        .collect())
}

fn reclaimable(
    git: &WorktreeGit,
    activity: &dyn SessionActivity,
    worktree: &WorktreeRef,
) -> Result<bool, WorktreeGitError> {
    if activity.is_active(&worktree.path)
        || worktree.branch.is_none()
        || git.is_dirty(&worktree.path)?
    {
        return Ok(false);
    }
    for submodule in git.submodule_paths()? {
        let path = worktree.path.join(submodule);
        if path.exists() && git.is_dirty(&path)? {
            return Ok(false);
        }
    }
    Ok(git.ahead_behind(&worktree.path, "main")?.0 == 0)
}

fn reclaim_order(
    left: &&WorktreeRef,
    right: &&WorktreeRef,
) -> std::cmp::Ordering {
    modified_at(&left.path)
        .cmp(&modified_at(&right.path))
        .then_with(|| left.name.cmp(&right.name))
}

fn modified_at(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn session_short_id(session_id: &str) -> String {
    session_id.chars().take(8).collect()
}

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.parse().ok()
}

fn session_record_is_active(
    path: &Path,
    worktree: &str,
    stale_after: Duration,
) -> bool {
    let Ok(record) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(record) = serde_json::from_str::<Value>(&record) else {
        return false;
    };
    let record_path = record
        .pointer("/metadata/worktree/path")
        .and_then(Value::as_str)
        .and_then(normalized_path);
    let timestamp = record
        .get("captured_at")
        .or_else(|| record.get("started_at"))
        .and_then(Value::as_str)
        .and_then(parse_timestamp);
    record_path.as_deref() == Some(worktree)
        && timestamp
            .is_some_and(|timestamp| timestamp_is_fresh(timestamp, stale_after))
}

fn normalized_path(path: impl AsRef<Path>) -> Option<String> {
    fs::canonicalize(path)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/").to_lowercase())
}

fn parse_timestamp(timestamp: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(timestamp, &Rfc3339).ok()
}

fn timestamp_is_fresh(
    timestamp: OffsetDateTime,
    stale_after: Duration,
) -> bool {
    let now = OffsetDateTime::now_utc();
    if timestamp > now {
        return true;
    }
    (now - timestamp).unsigned_abs() <= stale_after
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{
            Path,
            PathBuf,
        },
        process::Command,
        time::Duration,
    };

    use time::{
        OffsetDateTime,
        format_description::well_known::Rfc3339,
    };

    use super::{
        NeverActive,
        ProvisionError,
        ProvisionOutcome,
        ProvisionPolicy,
        SessionActivity,
        SessionStoreActivity,
        provision_for_session,
    };
    use crate::tests::Fixture;

    const SESSION_ID: &str = "12345678-1234-4234-8234-123456789abc";

    struct ActiveWorktree(PathBuf);

    impl SessionActivity for ActiveWorktree {
        fn is_active(
            &self,
            worktree: &Path,
        ) -> bool {
            worktree == self.0
        }
    }

    fn policy(max_worktrees: usize) -> ProvisionPolicy {
        ProvisionPolicy {
            max_worktrees,
            stale_after: Duration::from_secs(60),
            base_ref: "main".to_string(),
        }
    }

    fn commit(
        directory: &Path,
        message: &str,
    ) {
        let output = Command::new("git")
            .arg("-C")
            .arg(directory)
            .args([
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-am",
                message,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn assert_cap_reached(result: Result<ProvisionOutcome, ProvisionError>) {
        assert!(matches!(result, Err(ProvisionError::CapReached { .. })));
    }

    #[test]
    fn second_call_for_session_is_idempotent() {
        let fixture = Fixture::new();
        let git = fixture.git();
        let policy = policy(8);
        let first =
            provision_for_session(&git, &NeverActive, SESSION_ID, &policy)
                .unwrap();
        let expected = match first {
            ProvisionOutcome::Created(worktree) => worktree,
            other => panic!("expected creation, got {other:?}"),
        };

        let second =
            provision_for_session(&git, &NeverActive, SESSION_ID, &policy)
                .unwrap();
        assert!(matches!(
            second,
            ProvisionOutcome::AlreadyProvisioned(worktree) if worktree == expected
        ));
        assert_eq!(git.list_worktrees().unwrap().len(), 1);
    }

    #[test]
    fn creates_named_worktree_when_no_candidate_exists() {
        let fixture = Fixture::new();
        let outcome = provision_for_session(
            &fixture.git(),
            &NeverActive,
            SESSION_ID,
            &policy(8),
        )
        .unwrap();

        assert!(matches!(
            outcome,
            ProvisionOutcome::Created(worktree)
                if worktree.name == "12345678-session"
                    && worktree.branch.as_deref() == Some("agent/12345678-session")
        ));
    }

    #[test]
    fn reclaims_clean_inactive_worktree_and_preserves_marker() {
        let fixture = Fixture::new();
        fs::write(fixture.main.join(".git/info/exclude"), "marker.txt\n")
            .unwrap();
        let git = fixture.git();
        let old = git.create_worktree("old", "agent/old", "main").unwrap();
        fs::write(old.path.join("marker.txt"), "keep\n").unwrap();
        assert!(!git.is_dirty(&old.path).unwrap());

        let outcome =
            provision_for_session(&git, &NeverActive, SESSION_ID, &policy(1))
                .unwrap();
        let reclaimed = match outcome {
            ProvisionOutcome::Reclaimed {
                worktree,
                previous_name,
            } => {
                assert_eq!(previous_name, "old");
                worktree
            },
            other => panic!("expected reclaim, got {other:?}"),
        };

        assert!(!old.path.exists());
        assert_eq!(
            fs::read_to_string(reclaimed.path.join("marker.txt")).unwrap(),
            "keep\n"
        );
        assert!(!git.branch_exists("agent/old").unwrap());
        assert!(git.branch_exists("agent/12345678-session").unwrap());
    }

    #[test]
    fn dirty_worktree_is_not_reclaimed() {
        let fixture = Fixture::new();
        let git = fixture.git();
        let old = git.create_worktree("old", "agent/old", "main").unwrap();
        fs::write(old.path.join("untracked.txt"), "preserve\n").unwrap();

        assert_cap_reached(provision_for_session(
            &git,
            &NeverActive,
            SESSION_ID,
            &policy(1),
        ));
        assert!(old.path.exists());
        assert!(git.branch_exists("agent/old").unwrap());
        assert!(!git.branch_exists("agent/12345678-session").unwrap());
    }

    #[test]
    fn ahead_worktree_is_not_reclaimed() {
        let fixture = Fixture::new();
        let git = fixture.git();
        let old = git.create_worktree("old", "agent/old", "main").unwrap();
        fs::write(old.path.join("tracked.txt"), "advance\n").unwrap();
        commit(&old.path, "advance");
        assert_eq!(git.ahead_behind(&old.path, "main").unwrap().0, 1);

        assert_cap_reached(provision_for_session(
            &git,
            &NeverActive,
            SESSION_ID,
            &policy(1),
        ));
        assert!(old.path.exists());
        assert!(git.branch_exists("agent/old").unwrap());
        assert!(!git.branch_exists("agent/12345678-session").unwrap());
    }

    #[test]
    fn active_worktree_is_not_reclaimed() {
        let fixture = Fixture::new();
        let git = fixture.git();
        let old = git.create_worktree("old", "agent/old", "main").unwrap();
        let activity = ActiveWorktree(old.path.clone());

        assert_cap_reached(provision_for_session(
            &git,
            &activity,
            SESSION_ID,
            &policy(1),
        ));
        assert!(old.path.exists());
        assert!(git.branch_exists("agent/old").unwrap());
        assert!(!git.branch_exists("agent/12345678-session").unwrap());
    }

    #[test]
    fn cap_without_reclaim_candidate_returns_cap_reached() {
        let fixture = Fixture::new();
        let git = fixture.git();
        let old = git.create_worktree("old", "agent/old", "main").unwrap();
        fs::write(old.path.join("untracked.txt"), "preserve\n").unwrap();

        let result =
            provision_for_session(&git, &NeverActive, SESSION_ID, &policy(1));
        assert_cap_reached(result);
        assert_eq!(git.list_worktrees().unwrap().len(), 1);
        assert!(!fixture.main.join(".worktrees/12345678-session").exists());
    }

    #[test]
    fn cap_with_reclaim_candidate_reclaims_instead() {
        let fixture = Fixture::new();
        let git = fixture.git();
        git.create_worktree("old", "agent/old", "main").unwrap();

        let outcome =
            provision_for_session(&git, &NeverActive, SESSION_ID, &policy(1))
                .unwrap();
        assert!(matches!(outcome, ProvisionOutcome::Reclaimed { .. }));
        assert_eq!(git.list_worktrees().unwrap().len(), 1);
    }

    #[test]
    fn session_store_activity_honors_fresh_and_stale_records() {
        let fixture = Fixture::new();
        let git = fixture.git();
        let worktree = git.create_worktree("old", "agent/old", "main").unwrap();
        let record =
            fixture.main.join(".session/sessions/session/session.json");
        fs::create_dir_all(record.parent().unwrap()).unwrap();
        let fresh = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
        let worktree_path = worktree.path.to_string_lossy().replace('\\', "/");
        fs::write(
            &record,
            format!(
                r#"{{"captured_at":"{fresh}","metadata":{{"worktree":{{"path":"{}"}}}}}}"#,
                worktree_path
            ),
        )
        .unwrap();

        let activity = SessionStoreActivity::new(
            fixture.main.join(".session"),
            Duration::from_secs(60),
        );
        assert!(activity.is_active(&worktree.path));

        fs::write(
            &record,
            format!(
                r#"{{"captured_at":"2000-01-01T00:00:00Z","metadata":{{"worktree":{{"path":"{}"}}}}}}"#,
                worktree_path
            ),
        )
        .unwrap();
        assert!(!activity.is_active(&worktree.path));
    }
}
