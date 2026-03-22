use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox.invalid.repo_root: path does not contain a .git directory")]
    InvalidRepoRoot,
    #[error("sandbox.invalid.assignment_id")]
    InvalidAssignmentId,
    #[error("sandbox.git.command_failed: {command} (status={status}, stderr={stderr})")]
    GitCommandFailed {
        command: String,
        status: i32,
        stderr: String,
    },
    #[error("sandbox.io: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct SandboxSpec {
    pub repo_root: PathBuf,
    pub worktrees_root: PathBuf,
    pub assignment_id: String,
    pub base_branch: String,
    pub branch_prefix: String,
}

impl SandboxSpec {
    pub fn branch_name(&self) -> Result<String, SandboxError> {
        let sanitized = sanitize_assignment_id(&self.assignment_id)?;
        Ok(format!("{}/{}", self.branch_prefix, sanitized))
    }

    pub fn worktree_path(&self) -> Result<PathBuf, SandboxError> {
        let sanitized = sanitize_assignment_id(&self.assignment_id)?;
        Ok(self.worktrees_root.join(sanitized))
    }
}

#[derive(Debug, Clone)]
pub struct SandboxHandle {
    pub branch_name: String,
    pub worktree_path: PathBuf,
}

pub struct SandboxManager;

impl SandboxManager {
    pub fn provision(spec: &SandboxSpec) -> Result<SandboxHandle, SandboxError> {
        if !spec.repo_root.join(".git").exists() {
            return Err(SandboxError::InvalidRepoRoot);
        }

        std::fs::create_dir_all(&spec.worktrees_root)?;

        let branch_name = spec.branch_name()?;
        let worktree_path = spec.worktree_path()?;

        run_git(
            &spec.repo_root,
            [
                "worktree",
                "add",
                "-B",
                &branch_name,
                to_utf8_path(&worktree_path)?,
                &spec.base_branch,
            ],
        )?;

        Ok(SandboxHandle {
            branch_name,
            worktree_path,
        })
    }

    pub fn cleanup(spec: &SandboxSpec, handle: &SandboxHandle) -> Result<(), SandboxError> {
        if handle.worktree_path.exists() {
            run_git(
                &spec.repo_root,
                ["worktree", "remove", "--force", to_utf8_path(&handle.worktree_path)?],
            )?;
        }

        run_git(&spec.repo_root, ["branch", "-D", &handle.branch_name])?;
        Ok(())
    }
}

fn sanitize_assignment_id(raw: &str) -> Result<String, SandboxError> {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        let safe = ch.is_ascii_alphanumeric() || ch == '-' || ch == '_';
        out.push(if safe { ch } else { '-' });
    }

    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        return Err(SandboxError::InvalidAssignmentId);
    }

    Ok(trimmed)
}

fn to_utf8_path(path: &Path) -> Result<&str, SandboxError> {
    path.to_str().ok_or_else(|| {
        SandboxError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "path contains non-utf8 bytes",
        ))
    })
}

fn run_git<I, S>(repo_root: &Path, args: I) -> Result<(), SandboxError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args_vec: Vec<String> = args
        .into_iter()
        .map(|v| v.as_ref().to_string_lossy().to_string())
        .collect();

    let output = Command::new("git")
        .current_dir(repo_root)
        .args(&args_vec)
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    Err(SandboxError::GitCommandFailed {
        command: format!("git {}", args_vec.join(" ")),
        status: output.status.code().unwrap_or(-1),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{SandboxSpec, sanitize_assignment_id};
    use std::path::PathBuf;

    #[test]
    fn sanitize_assignment_replaces_unsafe_chars() {
        let value = sanitize_assignment_id("ticket/123:alpha").expect("sanitize");
        assert_eq!(value, "ticket-123-alpha");
    }

    #[test]
    fn branch_name_uses_prefix() {
        let spec = SandboxSpec {
            repo_root: PathBuf::from("."),
            worktrees_root: PathBuf::from(".worktrees"),
            assignment_id: "abc-123".to_string(),
            base_branch: "main".to_string(),
            branch_prefix: "tickets".to_string(),
        };

        assert_eq!(spec.branch_name().expect("branch name"), "tickets/abc-123");
    }
}
