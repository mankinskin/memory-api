use std::path::PathBuf;

use thiserror::Error;

use crate::execution::provider::{ProviderError, StartSubagentRequest, StartSubagentResponse, SubagentProvider};
use crate::execution::sandbox::{SandboxError, SandboxHandle, SandboxManager, SandboxSpec};

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("runner.sandbox: {0}")]
    Sandbox(#[from] SandboxError),
    #[error("runner.provider: {0}")]
    Provider(#[from] ProviderError),
    #[error("runner.cleanup_failed_after_provider_error: provider={provider_error}, cleanup={cleanup_error}")]
    CleanupAfterProviderError {
        provider_error: String,
        cleanup_error: SandboxError,
    },
}

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub repo_root: PathBuf,
    pub worktrees_root: PathBuf,
    pub base_branch: String,
    pub branch_prefix: String,
}

#[derive(Debug, Clone)]
pub struct AssignmentRunRequest {
    pub ticket_id: String,
    pub assignment_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub struct AssignmentRunReceipt {
    pub run_id: String,
    pub status: String,
    pub branch: String,
    pub worktree_path: PathBuf,
}

pub trait SandboxProvisioner {
    fn provision(&self, spec: &SandboxSpec) -> Result<SandboxHandle, SandboxError>;
    fn cleanup(&self, spec: &SandboxSpec, handle: &SandboxHandle) -> Result<(), SandboxError>;
}

pub struct GitSandboxProvisioner;

impl SandboxProvisioner for GitSandboxProvisioner {
    fn provision(&self, spec: &SandboxSpec) -> Result<SandboxHandle, SandboxError> {
        SandboxManager::provision(spec)
    }

    fn cleanup(&self, spec: &SandboxSpec, handle: &SandboxHandle) -> Result<(), SandboxError> {
        SandboxManager::cleanup(spec, handle)
    }
}

pub struct AssignmentRunner<P, S>
where
    P: SubagentProvider,
    S: SandboxProvisioner,
{
    provider: P,
    sandbox: S,
    config: RunnerConfig,
}

impl<P, S> AssignmentRunner<P, S>
where
    P: SubagentProvider,
    S: SandboxProvisioner,
{
    pub fn new(provider: P, sandbox: S, config: RunnerConfig) -> Self {
        Self {
            provider,
            sandbox,
            config,
        }
    }

    pub fn start_assignment(
        &self,
        request: &AssignmentRunRequest,
    ) -> Result<AssignmentRunReceipt, RunnerError> {
        let spec = SandboxSpec {
            repo_root: self.config.repo_root.clone(),
            worktrees_root: self.config.worktrees_root.clone(),
            assignment_id: request.assignment_id.clone(),
            base_branch: self.config.base_branch.clone(),
            branch_prefix: self.config.branch_prefix.clone(),
        };

        let handle = self.sandbox.provision(&spec)?;

        let provider_req = StartSubagentRequest {
            ticket_id: request.ticket_id.clone(),
            assignment_id: request.assignment_id.clone(),
            prompt: request.prompt.clone(),
            branch: handle.branch_name.clone(),
            worktree_path: handle.worktree_path.to_string_lossy().to_string(),
        };

        let provider_resp = self.provider.start_subagent(&provider_req);
        let StartSubagentResponse { run_id, status } = match provider_resp {
            Ok(resp) => resp,
            Err(err) => {
                if let Err(cleanup_error) = self.sandbox.cleanup(&spec, &handle) {
                    return Err(RunnerError::CleanupAfterProviderError {
                        provider_error: err.to_string(),
                        cleanup_error,
                    });
                }
                return Err(RunnerError::Provider(err));
            }
        };

        Ok(AssignmentRunReceipt {
            run_id,
            status,
            branch: handle.branch_name,
            worktree_path: handle.worktree_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use crate::execution::provider::{ProviderError, StartSubagentRequest, StartSubagentResponse, SubagentProvider};
    use crate::execution::runner::{
        AssignmentRunRequest, AssignmentRunner, GitSandboxProvisioner, RunnerConfig, RunnerError,
        SandboxProvisioner,
    };
    use crate::execution::sandbox::{SandboxError, SandboxHandle, SandboxSpec};

    struct FakeProvider {
        ok_response: Option<StartSubagentResponse>,
        err_response: Option<(u16, String)>,
        seen: Arc<Mutex<Vec<StartSubagentRequest>>>,
    }

    impl SubagentProvider for FakeProvider {
        fn start_subagent(
            &self,
            request: &StartSubagentRequest,
        ) -> Result<StartSubagentResponse, ProviderError> {
            self.seen.lock().expect("lock").push(request.clone());
            if let Some(resp) = &self.ok_response {
                return Ok(resp.clone());
            }
            if let Some((status, body)) = &self.err_response {
                return Err(ProviderError::UnexpectedStatus {
                    status: *status,
                    body: body.clone(),
                });
            }
            Err(ProviderError::UnexpectedStatus {
                status: 500,
                body: "missing fake provider response".to_string(),
            })
        }
    }

    struct FakeSandbox {
        handle: SandboxHandle,
        cleanup_ok: bool,
        cleanup_count: Arc<Mutex<usize>>,
    }

    impl SandboxProvisioner for FakeSandbox {
        fn provision(&self, _spec: &SandboxSpec) -> Result<SandboxHandle, SandboxError> {
            Ok(self.handle.clone())
        }

        fn cleanup(&self, _spec: &SandboxSpec, _handle: &SandboxHandle) -> Result<(), SandboxError> {
            let mut count = self.cleanup_count.lock().expect("lock");
            *count += 1;
            if self.cleanup_ok {
                Ok(())
            } else {
                Err(SandboxError::InvalidRepoRoot)
            }
        }
    }

    fn runner_config() -> RunnerConfig {
        RunnerConfig {
            repo_root: PathBuf::from("."),
            worktrees_root: PathBuf::from(".worktrees"),
            base_branch: "main".to_string(),
            branch_prefix: "tickets".to_string(),
        }
    }

    #[test]
    fn start_assignment_builds_provider_request_and_returns_receipt() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let provider = FakeProvider {
            ok_response: Some(StartSubagentResponse {
                run_id: "run-1".to_string(),
                status: "started".to_string(),
            }),
            err_response: None,
            seen: Arc::clone(&seen),
        };

        let sandbox = FakeSandbox {
            handle: SandboxHandle {
                branch_name: "tickets/abc-123".to_string(),
                worktree_path: PathBuf::from(".worktrees/abc-123"),
            },
            cleanup_ok: true,
            cleanup_count: Arc::new(Mutex::new(0)),
        };

        let runner = AssignmentRunner::new(provider, sandbox, runner_config());
        let req = AssignmentRunRequest {
            ticket_id: "t-1".to_string(),
            assignment_id: "abc-123".to_string(),
            prompt: "do the task".to_string(),
        };

        let receipt = runner.start_assignment(&req).expect("start assignment");
        assert_eq!(receipt.run_id, "run-1");
        assert_eq!(receipt.branch, "tickets/abc-123");

        let calls = seen.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].ticket_id, "t-1");
    }

    #[test]
    fn start_assignment_cleans_up_when_provider_fails() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let provider = FakeProvider {
            ok_response: None,
            err_response: Some((500, "boom".to_string())),
            seen,
        };
        let cleanup_count = Arc::new(Mutex::new(0));
        let sandbox = FakeSandbox {
            handle: SandboxHandle {
                branch_name: "tickets/abc-123".to_string(),
                worktree_path: PathBuf::from(".worktrees/abc-123"),
            },
            cleanup_ok: true,
            cleanup_count: Arc::clone(&cleanup_count),
        };

        let runner = AssignmentRunner::new(provider, sandbox, runner_config());
        let req = AssignmentRunRequest {
            ticket_id: "t-1".to_string(),
            assignment_id: "abc-123".to_string(),
            prompt: "do the task".to_string(),
        };

        let err = runner.start_assignment(&req).expect_err("expected provider error");
        assert!(matches!(err, RunnerError::Provider(_)));
        assert_eq!(*cleanup_count.lock().expect("lock"), 1);
    }

    #[test]
    fn start_assignment_reports_cleanup_failure() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let provider = FakeProvider {
            ok_response: None,
            err_response: Some((500, "boom".to_string())),
            seen,
        };

        let sandbox = FakeSandbox {
            handle: SandboxHandle {
                branch_name: "tickets/abc-123".to_string(),
                worktree_path: PathBuf::from(".worktrees/abc-123"),
            },
            cleanup_ok: false,
            cleanup_count: Arc::new(Mutex::new(0)),
        };

        let runner = AssignmentRunner::new(provider, sandbox, runner_config());
        let req = AssignmentRunRequest {
            ticket_id: "t-1".to_string(),
            assignment_id: "abc-123".to_string(),
            prompt: "do the task".to_string(),
        };

        let err = runner.start_assignment(&req).expect_err("expected cleanup error");
        assert!(matches!(err, RunnerError::CleanupAfterProviderError { .. }));
    }

    #[test]
    fn git_sandbox_type_is_constructible() {
        let _ = GitSandboxProvisioner;
    }
}