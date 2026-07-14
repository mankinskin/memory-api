use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    fs,
    io::ErrorKind,
    path::{
        Path,
        PathBuf,
    },
};
use uuid::Uuid;

use serde::{
    Deserialize,
    Serialize,
    de::DeserializeOwned,
};

use crate::{
    CopilotHookPayload,
    SESSION_SCHEMA_VERSION,
    SessionAuditReport,
    SessionAuditSelector,
    SessionCaptureRequest,
    SessionError,
    SessionFinishRecord,
    SessionFinishResult,
    SessionHandoffRecord,
    SessionHandoffResult,
    SessionLinks,
    SessionMetadata,
    SessionPinFeedbackSink,
    SessionPinnedEntity,
    SessionPinnedEntityHeader,
    SessionPinnedEntityKind,
    SessionRecord,
    SessionRunLineage,
    SessionRuntimeContext,
    SessionRuntimeInitRequest,
    SessionRuntimeInitResult,
    SessionRuntimeView,
    SessionTicketStateResolver,
    SessionTurn,
    SessionValidationGate,
    SessionWorkflowDiagnostic,
    SessionWorkflowEdge,
    SessionWorkflowEdgeKind,
    SessionWorkflowNode,
    SessionWorkflowNodeDraft,
    SessionWorkflowNodeResolution,
    SessionWorkflowNodeStatus,
    SessionWorkflowSnapshot,
    SessionWorktreeAllocationMode,
    SessionWorktreeAssignment,
    SessionWorktreeStatus,
    audit::build_session_audit_report,
    hook::{
        CopilotHookEvent,
        copilot_payload_from_transcript_path,
    },
    peek::{
        PromptPackOptions,
        SessionPromptPack,
        SessionSkeleton,
        SessionTurnRange,
        peek_prompt_pack,
        peek_skeleton,
        peek_turn_range,
    },
};
use test_api::{
    ExecutionQuery,
    TestStoreConfig,
    ValidationOutcome,
};
use ticket_api::storage::TicketStore;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSessionManifest {
    #[serde(default = "crate::default_session_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub source: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    pub metadata: SessionMetadata,
    #[serde(default)]
    pub links: SessionLinks,
}

impl From<&SessionRecord> for PersistedSessionManifest {
    fn from(record: &SessionRecord) -> Self {
        Self {
            schema_version: record.schema_version,
            session_id: record.session_id.clone(),
            source: record.source.clone(),
            started_at: record.started_at,
            captured_at: record.captured_at,
            metadata: record.metadata.clone(),
            links: record.links.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSessionTranscript {
    #[serde(default = "crate::default_session_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<SessionTurn>,
}

impl From<&SessionRecord> for PersistedSessionTranscript {
    fn from(record: &SessionRecord) -> Self {
        Self {
            schema_version: record.schema_version,
            session_id: record.session_id.clone(),
            captured_at: record.captured_at,
            turns: record.turns.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSessionEvents {
    #[serde(default = "crate::default_session_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<CopilotHookEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedActiveWorkspaceSession {
    pub workspace_session_id: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedRuntimeContext {
    #[serde(default = "crate::default_runtime_context_schema_version")]
    pub schema_version: u32,
    pub workspace_session_id: String,
    pub workspace_slug: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub active_run_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<SessionRunLineage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_entities: Vec<SessionPinnedEntity>,
    #[serde(default)]
    pub workflow: crate::SessionWorkflowGraph,
}

impl From<SessionRuntimeContext> for PersistedRuntimeContext {
    fn from(value: SessionRuntimeContext) -> Self {
        Self {
            schema_version: value.schema_version,
            workspace_session_id: value.workspace_session_id,
            workspace_slug: value.workspace_slug,
            created_at: value.created_at,
            updated_at: value.updated_at,
            active_run_id: value.active_run_id,
            runs: value.runs,
            pinned_entities: value.pinned_entities,
            workflow: value.workflow,
        }
    }
}

impl From<PersistedRuntimeContext> for SessionRuntimeContext {
    fn from(value: PersistedRuntimeContext) -> Self {
        Self {
            schema_version: value.schema_version,
            workspace_session_id: value.workspace_session_id,
            workspace_slug: value.workspace_slug,
            created_at: value.created_at,
            updated_at: value.updated_at,
            active_run_id: value.active_run_id,
            runs: value.runs,
            pinned_entities: value.pinned_entities,
            workflow: value.workflow,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStoreConfig {
    pub root: PathBuf,
    pub workspace_slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorktreeCheckInRequest {
    pub session_id: String,
    pub owner_id: String,
    pub ticket_id: String,
    pub worktree_path: PathBuf,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorktreeCheckInReceipt {
    pub session_id: String,
    pub owner_id: String,
    pub ticket_id: String,
    pub worktree_path: PathBuf,
    pub branch: String,
    pub allocation_mode: SessionWorktreeAllocationMode,
    pub status: SessionWorktreeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_path: Option<PathBuf>,
}

impl SessionStoreConfig {
    pub fn new(
        root: impl Into<PathBuf>,
        workspace_slug: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            workspace_slug: workspace_slug.into(),
        }
    }

    pub fn paths_for(
        &self,
        record: &SessionRecord,
    ) -> Result<SessionStorePaths, SessionError> {
        self.paths_for_session_id(&record.session_id)
    }

    pub fn capture_copilot_hook(
        &self,
        payload: CopilotHookPayload,
    ) -> Result<SessionStorePlan, SessionError> {
        self.persist_capture(SessionCaptureRequest::copilot(payload))
    }

    pub fn capture_copilot_transcript(
        &self,
        transcript_path: impl AsRef<Path>,
        trigger: impl Into<String>,
    ) -> Result<SessionStorePlan, SessionError> {
        let payload = copilot_payload_from_transcript_path(
            transcript_path,
            self.workspace_slug.clone(),
            Some(trigger.into()),
        )?;

        self.capture_copilot_hook(payload)
    }

    pub fn read_session(
        &self,
        session_id: &str,
    ) -> Result<SessionRecord, SessionError> {
        let paths = self.paths_for_session_id(session_id)?;
        let manifest: PersistedSessionManifest =
            read_json(&paths.manifest_path)?;
        ensure_supported_schema_version(
            &paths.manifest_path,
            manifest.schema_version,
        )?;
        let transcript: PersistedSessionTranscript =
            read_json(&paths.transcript_path)?;
        ensure_supported_schema_version(
            &paths.transcript_path,
            transcript.schema_version,
        )?;

        Ok(SessionRecord {
            schema_version: manifest.schema_version,
            session_id: manifest.session_id,
            source: manifest.source,
            started_at: manifest.started_at,
            captured_at: manifest.captured_at,
            metadata: manifest.metadata,
            turns: transcript.turns,
            links: manifest.links,
        })
    }

    pub fn query_sessions(
        &self,
        query: &SessionQuery,
    ) -> Result<Vec<SessionRecord>, SessionError> {
        let sessions_root = self.sessions_root()?;
        if !sessions_root.exists() {
            return Ok(vec![]);
        }

        let mut records = vec![];
        for entry in
            fs::read_dir(&sessions_root).map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?
        {
            let entry = entry.map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?;
            let file_type =
                entry.file_type().map_err(|source| SessionError::Io {
                    path: entry.path(),
                    source,
                })?;

            if !file_type.is_dir() {
                continue;
            }

            let session_id = entry.file_name().to_string_lossy().into_owned();
            let record = self.read_session(&session_id)?;
            if session_matches_query(&record, query) {
                records.push(record);
            }
        }

        records.sort_by(|left, right| {
            right
                .captured_at
                .cmp(&left.captured_at)
                .then_with(|| left.session_id.cmp(&right.session_id))
        });

        if let Some(limit) = query.limit {
            records.truncate(limit);
        }

        Ok(records)
    }

    pub fn latest_session_id(&self) -> Result<Option<String>, SessionError> {
        let sessions_root = self.sessions_root()?;
        if !sessions_root.exists() {
            return Ok(None);
        }

        let mut newest: Option<SessionRecord> = None;
        for entry in
            fs::read_dir(&sessions_root).map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?
        {
            let entry = entry.map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?;
            let file_type =
                entry.file_type().map_err(|source| SessionError::Io {
                    path: entry.path(),
                    source,
                })?;
            if !file_type.is_dir() {
                continue;
            }

            let session_id = entry.file_name().to_string_lossy().into_owned();
            let record = match self.read_session(&session_id) {
                Ok(record) => record,
                Err(SessionError::NotFound { .. }) => continue,
                Err(SessionError::Deserialize { .. }) => continue,
                Err(err) => return Err(err),
            };

            let replace = match newest.as_ref() {
                None => true,
                Some(current) =>
                    record.captured_at > current.captured_at
                        || (record.captured_at == current.captured_at
                            && record.session_id < current.session_id),
            };
            if replace {
                newest = Some(record);
            }
        }

        Ok(newest.map(|record| record.session_id))
    }

    pub fn session_audit(
        &self,
        selector: SessionAuditSelector,
    ) -> Result<SessionAuditReport, SessionError> {
        let session_id = match selector {
            SessionAuditSelector::SessionId(session_id) => session_id,
            SessionAuditSelector::Latest => self.latest_session_id()?.ok_or(
                SessionError::NoSessionsFound {
                    root: self.sessions_root()?,
                },
            )?,
        };

        let record = self.read_session(&session_id)?;
        let paths = self.paths_for_session_id(&session_id)?;
        let events: Option<PersistedSessionEvents> =
            read_json_if_exists(&paths.events_path)?;
        if let Some(events) = &events {
            ensure_supported_schema_version(
                &paths.events_path,
                events.schema_version,
            )?;
        }

        Ok(build_session_audit_report(&record, events.as_ref()))
    }

    pub fn check_in_worktree(
        &self,
        request: SessionWorktreeCheckInRequest,
    ) -> Result<SessionWorktreeCheckInReceipt, SessionError> {
        validate_worktree_request(&request)?;

        if let Ok(mut existing_record) = self.read_session(&request.session_id)
        {
            let existing_assignment =
                existing_record.metadata.worktree.clone().ok_or_else(|| {
                    SessionError::MissingWorktreeAssignment {
                        session_id: request.session_id.clone(),
                    }
                })?;

            if existing_record.metadata.agent_id.as_deref()
                != Some(request.owner_id.as_str())
                || existing_record.metadata.ticket_id.as_deref()
                    != Some(request.ticket_id.as_str())
            {
                return Err(SessionError::SessionOwnershipMismatch {
                    session_id: request.session_id,
                });
            }

            if can_reuse_assignment(&existing_assignment, &request) {
                existing_record.metadata.worktree =
                    Some(SessionWorktreeAssignment {
                        allocation_mode: SessionWorktreeAllocationMode::Reused,
                        ..existing_assignment
                    });
                existing_record.captured_at = chrono::Utc::now();
                self.persist_record(existing_record.clone())?;
                return receipt_from_record(&existing_record);
            }

            fs::create_dir_all(&request.worktree_path).map_err(|source| {
                SessionError::Io {
                    path: request.worktree_path.clone(),
                    source,
                }
            })?;
            self.ensure_no_active_worktree_conflict(
                &request.worktree_path,
                Some(request.session_id.as_str()),
            )?;

            existing_record.metadata.worktree =
                Some(SessionWorktreeAssignment {
                    path: request.worktree_path,
                    branch: request.branch,
                    allocation_mode: SessionWorktreeAllocationMode::Rotated,
                    status: SessionWorktreeStatus::Active,
                    predecessor_session_id: None,
                    predecessor_path: Some(existing_assignment.path),
                });
            existing_record.captured_at = chrono::Utc::now();
            self.persist_record(existing_record.clone())?;
            return receipt_from_record(&existing_record);
        }

        let mut predecessor_path = None;
        if let Some(predecessor_session_id) = &request.predecessor_session_id {
            let mut predecessor = self.read_session(predecessor_session_id)?;
            let predecessor_assignment =
                predecessor.metadata.worktree.clone().ok_or_else(|| {
                    SessionError::MissingWorktreeAssignment {
                        session_id: predecessor_session_id.clone(),
                    }
                })?;

            if predecessor_assignment.path == request.worktree_path {
                return Err(SessionError::CrossSessionReuseRequiresAdopt {
                    session_id: predecessor_session_id.clone(),
                    path: predecessor_assignment.path,
                });
            }

            predecessor_path = Some(predecessor_assignment.path.clone());
            predecessor.metadata.worktree = Some(SessionWorktreeAssignment {
                status: SessionWorktreeStatus::Superseded,
                ..predecessor_assignment
            });
            predecessor.captured_at = chrono::Utc::now();
            self.persist_record(predecessor)?;
        }

        fs::create_dir_all(&request.worktree_path).map_err(|source| {
            SessionError::Io {
                path: request.worktree_path.clone(),
                source,
            }
        })?;
        self.ensure_no_active_worktree_conflict(&request.worktree_path, None)?;

        let record = SessionRecord {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id: request.session_id,
            source: "session-worktree-check-in".to_string(),
            started_at: chrono::Utc::now(),
            captured_at: chrono::Utc::now(),
            metadata: SessionMetadata {
                workspace_slug: self.workspace_slug.clone(),
                conversation_id: None,
                agent_id: Some(request.owner_id),
                ticket_id: Some(request.ticket_id),
                model: None,
                trigger: Some("session-check-in".to_string()),
                producer: None,
                copilot_version: None,
                vscode_version: None,
                protocol_version: None,
                worktree: Some(SessionWorktreeAssignment {
                    path: request.worktree_path,
                    branch: request.branch,
                    allocation_mode: if request.predecessor_session_id.is_some()
                    {
                        SessionWorktreeAllocationMode::Rotated
                    } else {
                        SessionWorktreeAllocationMode::New
                    },
                    status: SessionWorktreeStatus::Active,
                    predecessor_session_id: request.predecessor_session_id,
                    predecessor_path,
                }),
            },
            turns: vec![],
            links: SessionLinks::default(),
        };
        self.persist_record(record.clone())?;
        receipt_from_record(&record)
    }

    pub fn lookup_worktree(
        &self,
        session_id: &str,
    ) -> Result<SessionWorktreeCheckInReceipt, SessionError> {
        let record = self.read_session(session_id)?;
        receipt_from_record(&record)
    }

    /// Return a bounded window of transcript turns for a persisted session.
    pub fn peek_range(
        &self,
        session_id: &str,
        start: usize,
        end: Option<usize>,
    ) -> Result<SessionTurnRange, SessionError> {
        let record = self.read_session(session_id)?;
        Ok(peek_turn_range(&record, start, end))
    }

    /// Return a body-stripped skeleton overview of a persisted session.
    pub fn peek_skeleton(
        &self,
        session_id: &str,
        preview_chars: usize,
    ) -> Result<SessionSkeleton, SessionError> {
        let record = self.read_session(session_id)?;
        Ok(peek_skeleton(&record, preview_chars))
    }

    /// Return a prompt-facing compact view of a persisted session transcript.
    pub fn peek_prompt_pack(
        &self,
        session_id: &str,
        options: PromptPackOptions,
    ) -> Result<SessionPromptPack, SessionError> {
        let record = self.read_session(session_id)?;
        Ok(peek_prompt_pack(&record, options))
    }

    pub fn init_runtime_context(
        &self,
        request: SessionRuntimeInitRequest,
    ) -> Result<SessionRuntimeInitResult, SessionError> {
        let now = chrono::Utc::now();
        let workspace_session_id =
            self.resolve_workspace_session_id(request.workspace_session_id)?;

        // Serialize lineage updates with every other runtime mutation and with
        // finish. Without the lock, a concurrent pin/workflow mutation (or a
        // second init/resume) could read the same context and clobber the run
        // lineage this call appends.
        let _lock = self.acquire_runtime_lock(&workspace_session_id)?;

        let mut created_workspace = false;
        let mut created_run = false;

        let mut context = match self.read_runtime_context(&workspace_session_id)
        {
            Ok(context) => context,
            Err(SessionError::RuntimeContextNotFound { .. }) => {
                created_workspace = true;
                created_run = true;
                let run = SessionRunLineage {
                    run_id: Uuid::new_v4().to_string(),
                    predecessor_run_id: request.predecessor_run_id.clone(),
                    started_at: now,
                };

                SessionRuntimeContext {
                    schema_version: crate::RUNTIME_CONTEXT_SCHEMA_VERSION,
                    workspace_session_id: workspace_session_id.clone(),
                    workspace_slug: self.workspace_slug.clone(),
                    created_at: now,
                    updated_at: now,
                    active_run_id: run.run_id.clone(),
                    runs: vec![run],
                    pinned_entities: vec![],
                    workflow: Default::default(),
                }
            },
            Err(err) => return Err(err),
        };

        if !created_workspace {
            let predecessor = request
                .predecessor_run_id
                .clone()
                .or_else(|| context.active_run().map(|run| run.run_id.clone()));

            if request.force_new_run || request.predecessor_run_id.is_some() {
                // Appending a new run is a lineage mutation; a finished workspace
                // is immutable, so reject it under the lock.
                self.ensure_workspace_not_finished(&workspace_session_id)?;
                let run = SessionRunLineage {
                    run_id: Uuid::new_v4().to_string(),
                    predecessor_run_id: predecessor,
                    started_at: now,
                };
                context.active_run_id = run.run_id.clone();
                context.runs.push(run);
                created_run = true;
            }

            context.updated_at = now;
        }

        self.persist_runtime_context(&context)?;
        self.persist_active_workspace_session(&workspace_session_id)?;

        let run = context.active_run().cloned().ok_or_else(|| {
            SessionError::RuntimeContextNotFound {
                workspace_session_id: workspace_session_id.clone(),
            }
        })?;

        Ok(SessionRuntimeInitResult {
            context,
            run,
            created_workspace,
            created_run,
        })
    }

    pub fn read_runtime_context(
        &self,
        workspace_session_id: &str,
    ) -> Result<SessionRuntimeContext, SessionError> {
        validate_runtime_workspace_id(workspace_session_id)?;
        let paths = self.runtime_paths_for_workspace(workspace_session_id)?;

        let persisted: PersistedRuntimeContext = read_json(&paths.context_path)
            .map_err(|err| match err {
                SessionError::NotFound { .. } =>
                    SessionError::RuntimeContextNotFound {
                        workspace_session_id: workspace_session_id.to_string(),
                    },
                other => other,
            })?;

        ensure_supported_schema_version(
            &paths.context_path,
            persisted.schema_version,
        )?;

        Ok(persisted.into())
    }

    /// Reject workflow/pin mutations once the workspace has a persisted finish
    /// record. Finished workspaces are immutable: this guarantees a finished
    /// workspace cannot silently drift into an incomplete state while still
    /// returning a stale success from `finish_workflow`.
    fn ensure_workspace_not_finished(
        &self,
        workspace_session_id: &str,
    ) -> Result<(), SessionError> {
        let paths = self.runtime_paths_for_workspace(workspace_session_id)?;
        if paths.finish_path.exists() {
            return Err(SessionError::WorkspaceFinished {
                workspace_session_id: workspace_session_id.to_string(),
            });
        }
        Ok(())
    }

    /// Begin a runtime mutation: acquire the exclusive lock and *then* verify the
    /// workspace is not finished.
    ///
    /// Ordering is load-bearing. The finished-check must run under the lock so it
    /// cannot race with `finish_workflow`. Consider two threads: if the check ran
    /// before the lock, a mutation could observe "not finished", block on the
    /// lock while finish commits its record and releases the lock, then acquire
    /// the lock and mutate a workspace that is now finished — silently drifting a
    /// finished workspace into an incomplete state. Checking after the lock is
    /// held closes that window: any mutation that wins the lock after finish
    /// observes the finish record and is rejected with [`SessionError::WorkspaceFinished`].
    fn begin_runtime_mutation(
        &self,
        workspace_session_id: &str,
    ) -> Result<RuntimeMutationLock, SessionError> {
        let lock = self.acquire_runtime_lock(workspace_session_id)?;
        self.ensure_workspace_not_finished(workspace_session_id)?;
        Ok(lock)
    }

    /// Acquire an exclusive lock over a workspace runtime context for the duration
    /// of a read-modify-write mutation. This prevents two concurrent mutations from
    /// both reading the same context and silently clobbering each other's write.
    /// The lock is a `create_new` lock file, which is atomic on Windows and Unix.
    /// A stale lock left behind by a crashed process is reclaimed once it exceeds
    /// [`RUNTIME_LOCK_STALE_SECS`]; a live conflict fails fast with an explicit
    /// [`SessionError::RuntimeMutationConflict`].
    fn acquire_runtime_lock(
        &self,
        workspace_session_id: &str,
    ) -> Result<RuntimeMutationLock, SessionError> {
        let paths = self.runtime_paths_for_workspace(workspace_session_id)?;
        fs::create_dir_all(&paths.workspace_dir).map_err(|source| {
            SessionError::Io {
                path: paths.workspace_dir.clone(),
                source,
            }
        })?;
        let lock_path = paths.workspace_dir.join(".context.lock");

        for _ in 0..2 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    let _ = file
                        .write_all(chrono::Utc::now().to_rfc3339().as_bytes());
                    let _ = file.sync_all();
                    return Ok(RuntimeMutationLock {
                        lock_path: lock_path.clone(),
                    });
                },
                Err(source) if source.kind() == ErrorKind::AlreadyExists => {
                    if runtime_lock_is_stale(&lock_path) {
                        // Reclaim a lock abandoned by a crashed process, then retry.
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    return Err(SessionError::RuntimeMutationConflict {
                        workspace_session_id: workspace_session_id.to_string(),
                    });
                },
                Err(source) => {
                    return Err(SessionError::Io {
                        path: lock_path.clone(),
                        source,
                    });
                },
            }
        }

        Err(SessionError::RuntimeMutationConflict {
            workspace_session_id: workspace_session_id.to_string(),
        })
    }

    pub fn pin_runtime_entity(
        &self,
        workspace_session_id: &str,
        entity_urn: &str,
        relation: Option<String>,
        reason: Option<String>,
    ) -> Result<SessionRuntimeContext, SessionError> {
        self.pin_runtime_entity_with_sink(
            workspace_session_id,
            entity_urn,
            relation,
            reason,
            None,
        )
    }

    pub fn pin_runtime_entity_with_sink(
        &self,
        workspace_session_id: &str,
        entity_urn: &str,
        relation: Option<String>,
        reason: Option<String>,
        feedback_sink: Option<&dyn SessionPinFeedbackSink>,
    ) -> Result<SessionRuntimeContext, SessionError> {
        let _lock = self.begin_runtime_mutation(workspace_session_id)?;
        let mut context = self.read_runtime_context(workspace_session_id)?;
        let now = chrono::Utc::now();
        let kind = parse_entity_urn_kind(entity_urn)?;

        if let Some(existing) = context.find_pin_mut(entity_urn) {
            existing.last_used_at = now;
            if relation.is_some() {
                existing.relation = relation.clone();
            }
            if reason.is_some() {
                existing.reason = reason.clone();
            }
        } else {
            context.pinned_entities.push(SessionPinnedEntity {
                urn: entity_urn.to_string(),
                kind,
                relation,
                reason,
                pinned_at: now,
                last_used_at: now,
            });
            context
                .pinned_entities
                .sort_by(|left, right| left.urn.cmp(&right.urn));
        }

        context.updated_at = now;
        self.persist_runtime_context(&context)?;

        if let Some(sink) = feedback_sink {
            let _ = sink.record_pin_usage(
                &context.workspace_session_id,
                &context.active_run_id,
                entity_urn,
            );
        }

        Ok(context)
    }

    pub fn unpin_runtime_entity(
        &self,
        workspace_session_id: &str,
        entity_urn: &str,
    ) -> Result<SessionRuntimeContext, SessionError> {
        parse_entity_urn_kind(entity_urn)?;
        let _lock = self.begin_runtime_mutation(workspace_session_id)?;
        let mut context = self.read_runtime_context(workspace_session_id)?;
        let changed = context.remove_pin(entity_urn);
        if changed {
            context.updated_at = chrono::Utc::now();
            self.persist_runtime_context(&context)?;
        }
        Ok(context)
    }

    pub fn view_runtime_context(
        &self,
        workspace_session_id: &str,
    ) -> Result<SessionRuntimeView, SessionError> {
        let context = self.read_runtime_context(workspace_session_id)?;
        let mut pinned_headers = context
            .pinned_entities
            .iter()
            .map(|pin| SessionPinnedEntityHeader {
                urn: pin.urn.clone(),
                kind: pin.kind,
                relation: pin.relation.clone(),
                reason: pin.reason.clone(),
            })
            .collect::<Vec<_>>();
        pinned_headers.sort_by(|left, right| left.urn.cmp(&right.urn));

        Ok(SessionRuntimeView {
            workspace_session_id: context.workspace_session_id,
            active_run_id: context.active_run_id,
            pinned_count: pinned_headers.len(),
            pinned_headers,
        })
    }

    pub fn workflow_add_node(
        &self,
        workspace_session_id: &str,
        draft: SessionWorkflowNodeDraft,
    ) -> Result<SessionRuntimeContext, SessionError> {
        if draft.title.trim().is_empty() {
            return Err(SessionError::InvalidHookInput(
                "workflow node title cannot be empty".to_string(),
            ));
        }

        if draft.kind == crate::SessionWorkflowNodeKind::Ticket {
            let ticket_urn = draft.ticket_urn.as_deref().ok_or_else(|| {
                SessionError::InvalidHookInput(
                    "ticket workflow node requires --ticket-urn".to_string(),
                )
            })?;
            let parsed = parse_entity_urn(ticket_urn)?;
            if parsed.kind != SessionPinnedEntityKind::Ticket {
                return Err(SessionError::InvalidHookInput(format!(
                    "ticket workflow node requires a ticket URN, got {}",
                    ticket_urn
                )));
            }
        } else if draft.ticket_urn.is_some() {
            return Err(SessionError::InvalidHookInput(
                "only ticket workflow nodes may set ticket_urn".to_string(),
            ));
        }

        let _lock = self.begin_runtime_mutation(workspace_session_id)?;
        let mut context = self.read_runtime_context(workspace_session_id)?;
        let node_id = draft
            .node_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        if context
            .workflow
            .nodes
            .iter()
            .any(|node| node.node_id == node_id)
        {
            return Ok(context);
        }

        let now = chrono::Utc::now();
        context.workflow.nodes.push(SessionWorkflowNode {
            node_id,
            kind: draft.kind,
            requirement: draft.requirement,
            status: SessionWorkflowNodeStatus::Pending,
            title: draft.title,
            created_at: now,
            updated_at: now,
            ticket_urn: draft.ticket_urn,
            cached_ticket_title: draft.cached_ticket_title,
            deferred_reason: None,
            validation_spec_id: draft.validation_spec_id,
        });
        sort_workflow_graph(&mut context.workflow);
        context.updated_at = now;
        self.persist_runtime_context(&context)?;
        Ok(context)
    }

    pub fn workflow_update_node_status(
        &self,
        workspace_session_id: &str,
        node_id: &str,
        status: SessionWorkflowNodeStatus,
        deferred_reason: Option<String>,
    ) -> Result<SessionRuntimeContext, SessionError> {
        let _lock = self.begin_runtime_mutation(workspace_session_id)?;
        let mut context = self.read_runtime_context(workspace_session_id)?;
        let node = context
            .workflow
            .nodes
            .iter_mut()
            .find(|node| node.node_id == node_id)
            .ok_or_else(|| {
                SessionError::InvalidHookInput(format!(
                    "unknown workflow node id: {node_id}"
                ))
            })?;

        node.status = status;
        node.deferred_reason = if status == SessionWorkflowNodeStatus::Deferred
        {
            deferred_reason
        } else {
            None
        };
        node.updated_at = chrono::Utc::now();
        context.updated_at = node.updated_at;
        self.persist_runtime_context(&context)?;
        Ok(context)
    }

    pub fn workflow_add_edge(
        &self,
        workspace_session_id: &str,
        from: &str,
        to: &str,
        kind: SessionWorkflowEdgeKind,
    ) -> Result<SessionRuntimeContext, SessionError> {
        let _lock = self.begin_runtime_mutation(workspace_session_id)?;
        let mut context = self.read_runtime_context(workspace_session_id)?;
        let known = context
            .workflow
            .nodes
            .iter()
            .map(|node| node.node_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if !known.contains(from) || !known.contains(to) {
            return Err(SessionError::InvalidHookInput(format!(
                "cannot link unknown workflow nodes: {from} -> {to}"
            )));
        }

        let edge = SessionWorkflowEdge {
            from: from.to_string(),
            to: to.to_string(),
            kind,
        };
        if !context
            .workflow
            .edges
            .iter()
            .any(|existing| existing == &edge)
        {
            context.workflow.edges.push(edge);
            sort_workflow_graph(&mut context.workflow);
            context.updated_at = chrono::Utc::now();
            self.persist_runtime_context(&context)?;
        }
        Ok(context)
    }

    pub fn workflow_promote_node_to_ticket(
        &self,
        workspace_session_id: &str,
        node_id: &str,
        ticket_urn: &str,
        cached_ticket_title: Option<String>,
    ) -> Result<SessionRuntimeContext, SessionError> {
        let parsed = parse_entity_urn(ticket_urn)?;
        if parsed.kind != SessionPinnedEntityKind::Ticket {
            return Err(SessionError::InvalidHookInput(format!(
                "promotion requires a ticket URN, got {ticket_urn}"
            )));
        }
        let _lock = self.begin_runtime_mutation(workspace_session_id)?;
        let mut context = self.read_runtime_context(workspace_session_id)?;
        let node = context
            .workflow
            .nodes
            .iter_mut()
            .find(|node| node.node_id == node_id)
            .ok_or_else(|| {
                SessionError::InvalidHookInput(format!(
                    "unknown workflow node id: {node_id}"
                ))
            })?;

        node.kind = crate::SessionWorkflowNodeKind::Ticket;
        node.ticket_urn = Some(ticket_urn.to_string());
        if cached_ticket_title.is_some() {
            node.cached_ticket_title = cached_ticket_title;
        }
        node.updated_at = chrono::Utc::now();
        context.updated_at = node.updated_at;
        self.persist_runtime_context(&context)?;
        Ok(context)
    }

    pub fn workflow_snapshot(
        &self,
        workspace_session_id: &str,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<SessionWorkflowSnapshot, SessionError> {
        let context = self.read_runtime_context(workspace_session_id)?;
        let mut resolutions = Vec::new();
        let mut diagnostics = Vec::new();
        let owned_resolver = if resolver.is_none() {
            Some(self.default_ticket_state_resolver()?)
        } else {
            None
        };
        let resolver = resolver.or(owned_resolver
            .as_ref()
            .map(|item| item as &dyn SessionTicketStateResolver));

        if let Some(resolver) = resolver {
            for node in &context.workflow.nodes {
                let Some(ticket_urn) = node.ticket_urn.as_deref() else {
                    continue;
                };

                match resolver.resolve_ticket_state(ticket_urn) {
                    Ok(state) =>
                        resolutions.push(SessionWorkflowNodeResolution {
                            node_id: node.node_id.clone(),
                            live_ticket_state: state,
                        }),
                    Err(message) =>
                        diagnostics.push(SessionWorkflowDiagnostic {
                            node_id: node.node_id.clone(),
                            code: "ticket-state-unavailable".to_string(),
                            message,
                        }),
                }
            }
        }

        Ok(SessionWorkflowSnapshot {
            workflow: context.workflow,
            resolutions,
            diagnostics,
        })
    }

    pub fn workflow_render_terminal(
        &self,
        workspace_session_id: &str,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<String, SessionError> {
        let snapshot =
            self.workflow_snapshot(workspace_session_id, resolver)?;
        let mut lines = Vec::new();
        lines.push(format!("workflow {}", workspace_session_id));

        let live_states = snapshot
            .resolutions
            .iter()
            .map(|item| (item.node_id.clone(), item.live_ticket_state.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut blockers = BTreeMap::<String, Vec<String>>::new();
        for edge in &snapshot.workflow.edges {
            if edge.kind != SessionWorkflowEdgeKind::DependsOn {
                continue;
            }
            if let Some(dependency) = snapshot
                .workflow
                .nodes
                .iter()
                .find(|node| node.node_id == edge.to)
            {
                if !node_is_effectively_done(
                    dependency,
                    live_states.get(&dependency.node_id),
                ) {
                    blockers
                        .entry(edge.from.clone())
                        .or_default()
                        .push(edge.to.clone());
                }
            }
        }

        for node in &snapshot.workflow.nodes {
            let requirement = match node.requirement {
                crate::SessionWorkflowNodeRequirement::Required => "required",
                crate::SessionWorkflowNodeRequirement::Optional => "optional",
            };
            let live_state = live_states
                .get(&node.node_id)
                .and_then(|state| state.as_deref())
                .unwrap_or("-");
            let blockers_for_node = blockers
                .get(&node.node_id)
                .cloned()
                .unwrap_or_default()
                .join(",");
            let blocker_view = if blockers_for_node.is_empty() {
                "-".to_string()
            } else {
                blockers_for_node
            };

            lines.push(format!(
                "- {} [{} {}] ticket_state={} blockers={} {}",
                node.node_id,
                requirement,
                workflow_status_label(node.status),
                live_state,
                blocker_view,
                node.title
            ));
        }

        for diag in &snapshot.diagnostics {
            lines.push(format!(
                "! {} {} {}",
                diag.node_id, diag.code, diag.message
            ));
        }

        Ok(lines.join("\n"))
    }

    pub fn workflow_render_mermaid(
        &self,
        workspace_session_id: &str,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<String, SessionError> {
        let snapshot =
            self.workflow_snapshot(workspace_session_id, resolver)?;
        let live_states = snapshot
            .resolutions
            .iter()
            .map(|item| (item.node_id.clone(), item.live_ticket_state.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut lines = vec!["flowchart TD".to_string()];
        for node in &snapshot.workflow.nodes {
            let req = match node.requirement {
                crate::SessionWorkflowNodeRequirement::Required => "req",
                crate::SessionWorkflowNodeRequirement::Optional => "opt",
            };
            let live = live_states
                .get(&node.node_id)
                .and_then(|state| state.as_deref())
                .unwrap_or("-");
            let label = format!(
                "{} |{}| |{}| |ticket:{}|",
                node.title,
                req,
                workflow_status_label(node.status),
                live
            );
            lines.push(format!(
                "  {}[\"{}\"]",
                mermaid_node_id(&node.node_id),
                escape_mermaid_label(&label)
            ));
        }

        for edge in &snapshot.workflow.edges {
            let arrow = match edge.kind {
                SessionWorkflowEdgeKind::DependsOn => "-->|depends_on|",
                SessionWorkflowEdgeKind::Order => "-->|order|",
            };
            lines.push(format!(
                "  {} {} {}",
                mermaid_node_id(&edge.from),
                arrow,
                mermaid_node_id(&edge.to)
            ));
        }

        for diag in &snapshot.diagnostics {
            let diag_id = format!("diag_{}", mermaid_node_id(&diag.node_id));
            lines.push(format!(
                "  {}((\"{}\"))",
                diag_id,
                escape_mermaid_label(&format!(
                    "{}: {}",
                    diag.code, diag.message
                ))
            ));
            lines.push(format!(
                "  {} -.-> {}",
                diag_id,
                mermaid_node_id(&diag.node_id)
            ));
        }

        Ok(lines.join("\n"))
    }

    pub fn create_handoff_record(
        &self,
        workspace_session_id: &str,
        validation: Vec<SessionValidationGate>,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<SessionHandoffRecord, SessionError> {
        let context = self.read_runtime_context(workspace_session_id)?;
        let workflow =
            self.workflow_snapshot(workspace_session_id, resolver)?;
        let validation =
            self.resolve_validation_gates(&context, validation, false)?;
        let view = self.view_runtime_context(workspace_session_id)?;
        let handoff_id = Uuid::new_v4().to_string();
        let resume_command = format!(
            "session-cli resume --workspace-session-id {} --predecessor-run-id {}",
            context.workspace_session_id, context.active_run_id
        );
        let record = SessionHandoffRecord {
            handoff_id: handoff_id.clone(),
            workspace_session_id: context.workspace_session_id.clone(),
            outgoing_run_id: context.active_run_id,
            created_at: chrono::Utc::now(),
            resume_command,
            pinned_entities: view.pinned_headers,
            workflow,
            validation,
        };

        let paths = self.runtime_paths_for_workspace(workspace_session_id)?;
        fs::create_dir_all(&paths.handoffs_dir).map_err(|source| {
            SessionError::Io {
                path: paths.handoffs_dir.clone(),
                source,
            }
        })?;
        let handoff_path =
            paths.handoffs_dir.join(format!("{handoff_id}.json"));
        write_json(&handoff_path, &record)?;
        Ok(record)
    }

    pub fn create_handoff_result(
        &self,
        workspace_session_id: &str,
        validation: Vec<SessionValidationGate>,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<SessionHandoffResult, SessionError> {
        let record = self.create_handoff_record(
            workspace_session_id,
            validation,
            resolver,
        )?;
        let paths = self.runtime_paths_for_workspace(workspace_session_id)?;
        let record_path = paths
            .handoffs_dir
            .join(format!("{}.json", record.handoff_id));
        Ok(SessionHandoffResult {
            render: render_handoff_record_terminal(&record),
            record,
            record_path: record_path.to_string_lossy().into_owned(),
        })
    }

    pub fn render_handoff_terminal(
        &self,
        workspace_session_id: &str,
        validation: Vec<SessionValidationGate>,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<String, SessionError> {
        let result = self.create_handoff_result(
            workspace_session_id,
            validation,
            resolver,
        )?;
        Ok(result.render)
    }

    pub fn resume_workspace_context(
        &self,
        workspace_session_id: &str,
        predecessor_run_id: &str,
    ) -> Result<SessionRuntimeInitResult, SessionError> {
        let init = self.init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some(workspace_session_id.to_string()),
            predecessor_run_id: Some(predecessor_run_id.to_string()),
            force_new_run: true,
        })?;

        if init.run.run_id == predecessor_run_id {
            return Err(SessionError::InvalidHookInput(
                "resume must produce a new run id".to_string(),
            ));
        }
        Ok(init)
    }

    pub fn finish_workflow(
        &self,
        workspace_session_id: &str,
        validation: Vec<SessionValidationGate>,
        deferred_optional_node_ids: Vec<String>,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<SessionFinishResult, SessionError> {
        let paths = self.runtime_paths_for_workspace(workspace_session_id)?;
        if let Some(result) = Self::existing_finish_result(&paths.finish_path)?
        {
            return Ok(result);
        }

        // Hold the mutation lock across evaluation and finish-record write so a
        // concurrent workflow mutation cannot interleave with finish.
        let _lock = self.acquire_runtime_lock(workspace_session_id)?;
        // Re-check under the lock: another finish may have won the race.
        if let Some(result) = Self::existing_finish_result(&paths.finish_path)?
        {
            return Ok(result);
        }

        let context = self.read_runtime_context(workspace_session_id)?;
        let snapshot =
            self.workflow_snapshot(workspace_session_id, resolver)?;
        let deferred = deferred_optional_node_ids
            .into_iter()
            .collect::<BTreeSet<_>>();

        Self::evaluate_workflow_completion(&context, &snapshot, &deferred)?;
        let validation =
            self.evaluate_required_validation(&context, validation)?;

        let record = SessionFinishRecord {
            workspace_session_id: workspace_session_id.to_string(),
            run_id: context.active_run_id,
            finished_at: chrono::Utc::now(),
            deferred_optional_node_ids: deferred.into_iter().collect(),
            validation,
        };

        if let Some(parent) = paths.finish_path.parent() {
            fs::create_dir_all(parent).map_err(|source| SessionError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        write_json(&paths.finish_path, &record)?;
        Ok(SessionFinishResult {
            record,
            already_finished: false,
        })
    }

    /// Load a persisted finish record (if any) as an idempotent finish result.
    ///
    /// Extracted so `finish_workflow` can perform the pre-lock fast path and the
    /// under-lock re-check with a single branch each instead of duplicating the
    /// read-and-wrap logic inline.
    fn existing_finish_result(
        finish_path: &Path
    ) -> Result<Option<SessionFinishResult>, SessionError> {
        Ok(
            read_json_if_exists::<SessionFinishRecord>(finish_path)?.map(
                |record| SessionFinishResult {
                    record,
                    already_finished: true,
                },
            ),
        )
    }

    /// Pure completion predicate for finish: verify every required workflow node
    /// is done and every optional node is done or explicitly deferred with a
    /// reason. Ticket nodes whose live state could not be resolved fail closed.
    ///
    /// Extracted from `finish_workflow` so the completion invariant can be unit
    /// tested in isolation and so the locked finish path reads as a linear
    /// sequence rather than an oversized branching function.
    fn evaluate_workflow_completion(
        context: &SessionRuntimeContext,
        snapshot: &SessionWorkflowSnapshot,
        deferred: &BTreeSet<String>,
    ) -> Result<(), SessionError> {
        let live_states = snapshot
            .resolutions
            .iter()
            .map(|item| (item.node_id.clone(), item.live_ticket_state.clone()))
            .collect::<BTreeMap<_, _>>();
        // Ticket-state diagnostics (unavailable / misrouted / not-found) must be
        // able to block finish; they cannot be silently ignored.
        let diagnostics_by_node = snapshot
            .diagnostics
            .iter()
            .map(|diag| (diag.node_id.clone(), diag.message.clone()))
            .collect::<BTreeMap<_, _>>();

        for node in &context.workflow.nodes {
            // A required ticket-backed node whose live state could not be resolved
            // (missing, misrouted, or otherwise unavailable) fails closed with an
            // explicit unavailable reason instead of a generic "not done".
            if node.requirement
                == crate::SessionWorkflowNodeRequirement::Required
                && node.kind == crate::SessionWorkflowNodeKind::Ticket
            {
                if let Some(message) = diagnostics_by_node.get(&node.node_id) {
                    return Err(SessionError::FinishBlocked {
                        reason: format!(
                            "required ticket node {} has unavailable live state: {}",
                            node.node_id, message
                        ),
                    });
                }
            }

            let is_done =
                node_is_effectively_done(node, live_states.get(&node.node_id));
            if node.requirement
                == crate::SessionWorkflowNodeRequirement::Required
                && !is_done
            {
                return Err(SessionError::FinishBlocked {
                    reason: format!(
                        "required node {} is not done",
                        node.node_id
                    ),
                });
            }

            if node.requirement
                == crate::SessionWorkflowNodeRequirement::Optional
                && !is_done
            {
                let valid_defer = node.status
                    == SessionWorkflowNodeStatus::Deferred
                    && node.deferred_reason.is_some()
                    && deferred.contains(&node.node_id);
                if !valid_defer {
                    return Err(SessionError::FinishBlocked {
                        reason: format!(
                            "optional node {} must be deferred with a reason",
                            node.node_id
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    /// Resolve required validation gates from the authoritative test store and
    /// verify each required gate passed, returning the merged gate list to
    /// persist. Extracted from `finish_workflow` to keep authoritative-gate
    /// evaluation testable independently of the locked finish sequence.
    fn evaluate_required_validation(
        &self,
        context: &SessionRuntimeContext,
        validation: Vec<SessionValidationGate>,
    ) -> Result<Vec<SessionValidationGate>, SessionError> {
        let validation =
            self.resolve_validation_gates(context, validation, true)?;
        for gate in &validation {
            if gate.required && gate.outcome.as_deref() != Some("passed") {
                return Err(SessionError::FinishBlocked {
                    reason: format!(
                        "required validation {} is not passed",
                        gate.validation_spec_id
                    ),
                });
            }
        }
        Ok(validation)
    }

    fn resolve_validation_gates(
        &self,
        context: &SessionRuntimeContext,
        validation: Vec<SessionValidationGate>,
        strict_required: bool,
    ) -> Result<Vec<SessionValidationGate>, SessionError> {
        let mut by_id = BTreeMap::<String, SessionValidationGate>::new();
        for gate in validation {
            by_id.insert(gate.validation_spec_id.clone(), gate);
        }

        let required_specs = context
            .workflow
            .nodes
            .iter()
            .filter(|node| {
                node.kind == crate::SessionWorkflowNodeKind::Validation
                    && node.requirement
                        == crate::SessionWorkflowNodeRequirement::Required
            })
            .map(|node| {
                node.validation_spec_id.clone().ok_or_else(|| {
                    SessionError::FinishBlocked {
                        reason: format!(
                            "required validation node {} is missing validation_spec_id",
                            node.node_id
                        ),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if required_specs.is_empty() {
            return Ok(by_id.into_values().collect());
        }

        let test_store = self.test_store_config();
        for spec_id in required_specs {
            // Fail closed when a required guard references an unknown validation spec.
            test_store.get_spec(&spec_id).map_err(|error| {
                SessionError::FinishBlocked {
                    reason: format!(
                        "required validation {} is unavailable: {}",
                        spec_id, error
                    ),
                }
            })?;

            // Required outcomes are ALWAYS derived from the authoritative test-api
            // execution record. Caller-provided outcomes are never accepted as
            // completion authority; they may only identify or display a gate.
            let latest = test_store
                .list_executions(&ExecutionQuery {
                    validation_spec_id: Some(spec_id.clone()),
                    limit: Some(1),
                    ..ExecutionQuery::default()
                })
                .map_err(|error| SessionError::FinishBlocked {
                    reason: format!(
                        "required validation {} could not be queried: {}",
                        spec_id, error
                    ),
                })?;
            let outcome = latest
                .into_iter()
                .next()
                .map(|execution| validation_outcome_label(execution.outcome));

            // Fail closed for absent executions, failed executions, and blocked
            // executions when finish requires strict authoritative evidence.
            if strict_required && outcome.as_deref() != Some("passed") {
                return Err(SessionError::FinishBlocked {
                    reason: format!(
                        "required validation {} is not passed (authoritative outcome: {})",
                        spec_id,
                        outcome.as_deref().unwrap_or("no execution record")
                    ),
                });
            }

            by_id.insert(
                spec_id.clone(),
                SessionValidationGate {
                    validation_spec_id: spec_id,
                    required: true,
                    outcome,
                },
            );
        }

        Ok(by_id.into_values().collect())
    }

    pub(crate) fn paths_for_session_id(
        &self,
        session_id: &str,
    ) -> Result<SessionStorePaths, SessionError> {
        if self.root.as_os_str().is_empty() {
            return Err(SessionError::EmptyStoreRoot);
        }
        validate_segment(session_id, false)?;

        let session_dir = self.root.join("sessions").join(session_id);
        let manifest_path = session_dir.join("session.json");
        let transcript_path = session_dir.join("transcript.json");
        let events_path = session_dir.join("events.json");

        if manifest_path.parent().is_none()
            || transcript_path.parent().is_none()
            || events_path.parent().is_none()
        {
            return Err(SessionError::InvalidStorePath(session_dir));
        }

        Ok(SessionStorePaths {
            session_dir,
            manifest_path,
            transcript_path,
            events_path,
        })
    }

    fn sessions_root(&self) -> Result<PathBuf, SessionError> {
        if self.root.as_os_str().is_empty() {
            return Err(SessionError::EmptyStoreRoot);
        }
        Ok(self.root.join("sessions"))
    }

    fn runtime_root(&self) -> Result<PathBuf, SessionError> {
        if self.root.as_os_str().is_empty() {
            return Err(SessionError::EmptyStoreRoot);
        }
        Ok(self.root.join("runtime"))
    }

    fn active_workspace_session_path(&self) -> Result<PathBuf, SessionError> {
        Ok(self.runtime_root()?.join("active_workspace_session.json"))
    }

    fn runtime_paths_for_workspace(
        &self,
        workspace_session_id: &str,
    ) -> Result<SessionRuntimePaths, SessionError> {
        validate_runtime_workspace_id(workspace_session_id)?;
        let workspace_dir = self
            .runtime_root()?
            .join("workspaces")
            .join(workspace_session_id);
        let context_path = workspace_dir.join("context.json");
        let handoffs_dir = workspace_dir.join("handoffs");
        let finish_path = workspace_dir.join("finish.json");
        Ok(SessionRuntimePaths {
            workspace_dir,
            context_path,
            handoffs_dir,
            finish_path,
        })
    }

    fn persist_runtime_context(
        &self,
        context: &SessionRuntimeContext,
    ) -> Result<(), SessionError> {
        let paths =
            self.runtime_paths_for_workspace(&context.workspace_session_id)?;
        fs::create_dir_all(&paths.workspace_dir).map_err(|source| {
            SessionError::Io {
                path: paths.workspace_dir.clone(),
                source,
            }
        })?;

        write_json(
            &paths.context_path,
            &PersistedRuntimeContext::from(context.clone()),
        )
    }

    fn ticket_store_root(&self) -> PathBuf {
        sibling_store_root(&self.root, ".ticket")
    }

    fn test_store_root(&self) -> PathBuf {
        sibling_store_root(&self.root, ".test")
    }

    fn test_store_config(&self) -> TestStoreConfig {
        TestStoreConfig::new(
            self.test_store_root(),
            self.workspace_slug.clone(),
        )
    }

    fn default_ticket_state_resolver(
        &self
    ) -> Result<DefaultTicketStateResolver, SessionError> {
        let store = TicketStore::open_or_init(&self.ticket_store_root())
            .map_err(|error| {
                SessionError::InvalidHookInput(format!(
                    "ticket store resolver unavailable: {error}"
                ))
            })?;
        Ok(DefaultTicketStateResolver {
            store,
            workspace_slug: self.workspace_slug.clone(),
        })
    }

    fn resolve_workspace_session_id(
        &self,
        requested: Option<String>,
    ) -> Result<String, SessionError> {
        if let Some(id) = requested {
            validate_runtime_workspace_id(&id)?;
            return Ok(id);
        }

        let active_path = self.active_workspace_session_path()?;
        if let Some(active) = read_json_if_exists::<
            PersistedActiveWorkspaceSession,
        >(&active_path)?
        {
            validate_runtime_workspace_id(&active.workspace_session_id)?;
            return Ok(active.workspace_session_id);
        }

        Ok(Uuid::new_v4().to_string())
    }

    fn persist_active_workspace_session(
        &self,
        workspace_session_id: &str,
    ) -> Result<(), SessionError> {
        validate_runtime_workspace_id(workspace_session_id)?;
        let path = self.active_workspace_session_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| SessionError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        write_json(
            &path,
            &PersistedActiveWorkspaceSession {
                workspace_session_id: workspace_session_id.to_string(),
                updated_at: chrono::Utc::now(),
            },
        )
    }

    pub fn plan_capture(
        &self,
        request: SessionCaptureRequest,
    ) -> Result<SessionStorePlan, SessionError> {
        let (record, events) = request.into_record_and_events()?;
        let paths = self.paths_for(&record)?;
        let events = if events.is_empty() {
            None
        } else {
            Some(PersistedSessionEvents {
                schema_version: record.schema_version,
                session_id: record.session_id.clone(),
                captured_at: record.captured_at,
                events,
            })
        };
        Ok(SessionStorePlan {
            record,
            paths,
            events,
        })
    }

    pub fn persist_capture(
        &self,
        request: SessionCaptureRequest,
    ) -> Result<SessionStorePlan, SessionError> {
        let plan = self.plan_capture(request)?;
        plan.persist()?;
        Ok(plan)
    }

    fn persist_record(
        &self,
        record: SessionRecord,
    ) -> Result<SessionStorePlan, SessionError> {
        let paths = self.paths_for(&record)?;
        let plan = SessionStorePlan {
            record,
            paths,
            events: None,
        };
        plan.persist()?;
        Ok(plan)
    }

    fn ensure_no_active_worktree_conflict(
        &self,
        requested_path: &Path,
        ignored_session_id: Option<&str>,
    ) -> Result<(), SessionError> {
        for record in self.query_sessions(&SessionQuery::default())? {
            if ignored_session_id == Some(record.session_id.as_str()) {
                continue;
            }

            let Some(worktree) = record.metadata.worktree.as_ref() else {
                continue;
            };

            if worktree.status == SessionWorktreeStatus::Active
                && worktree.path == requested_path
            {
                return Err(SessionError::WorktreeConflict {
                    path: requested_path.to_path_buf(),
                    session_id: record.session_id,
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStorePaths {
    pub session_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub transcript_path: PathBuf,
    pub events_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRuntimePaths {
    pub workspace_dir: PathBuf,
    pub context_path: PathBuf,
    pub handoffs_dir: PathBuf,
    pub finish_path: PathBuf,
}

fn validate_runtime_workspace_id(value: &str) -> Result<(), SessionError> {
    if value.trim().is_empty() {
        return Err(SessionError::InvalidWorkspaceSessionId(value.to_string()));
    }
    let invalid = ['/', '\\', ':'];
    if value.chars().any(|ch| invalid.contains(&ch)) {
        return Err(SessionError::InvalidWorkspaceSessionId(value.to_string()));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedEntityUrn {
    workspace_slug: String,
    kind: SessionPinnedEntityKind,
    entity_id: String,
}

fn parse_entity_urn(entity_urn: &str) -> Result<ParsedEntityUrn, SessionError> {
    let trimmed = entity_urn.trim();
    if !trimmed.starts_with("ce://") {
        return Err(SessionError::InvalidEntityUrn(trimmed.to_string()));
    }

    let rest = trimmed.trim_start_matches("ce://");
    let mut segments = rest.split('/');
    let workspace_slug = segments.next().unwrap_or_default().to_string();
    let store = segments.next().unwrap_or_default();
    let entity_id = segments.next().unwrap_or_default().to_string();
    if workspace_slug.trim().is_empty() || entity_id.trim().is_empty() {
        return Err(SessionError::InvalidEntityUrn(trimmed.to_string()));
    }
    if segments.next().is_some() {
        return Err(SessionError::InvalidEntityUrn(trimmed.to_string()));
    }

    let kind = match store {
        "ticket" | "tickets" => SessionPinnedEntityKind::Ticket,
        "spec" | "specs" => SessionPinnedEntityKind::Spec,
        "rule" | "rules" => SessionPinnedEntityKind::Rule,
        _ => return Err(SessionError::InvalidEntityUrn(trimmed.to_string())),
    };

    Ok(ParsedEntityUrn {
        workspace_slug,
        kind,
        entity_id,
    })
}

fn parse_entity_urn_kind(
    entity_urn: &str
) -> Result<SessionPinnedEntityKind, SessionError> {
    Ok(parse_entity_urn(entity_urn)?.kind)
}

fn sibling_store_root(
    session_store_root: &Path,
    sibling_store_dir: &str,
) -> PathBuf {
    if session_store_root
        .file_name()
        .and_then(|name| name.to_str())
        == Some(".session")
    {
        if let Some(parent) = session_store_root.parent() {
            return parent.join(sibling_store_dir);
        }
    }
    session_store_root.join(sibling_store_dir)
}

/// Runtime mutation locks older than this are treated as abandoned by a crashed
/// process and reclaimed. Mutations are short read-modify-write critical sections,
/// so a lock held longer than this almost certainly outlived its owner.
const RUNTIME_LOCK_STALE_SECS: i64 = 30;

/// RAII guard that releases the runtime mutation lock file on drop.
struct RuntimeMutationLock {
    lock_path: PathBuf,
}

impl Drop for RuntimeMutationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

fn runtime_lock_is_stale(lock_path: &Path) -> bool {
    // Prefer the timestamp written into the lock file; fall back to the file's
    // modified time. If neither can be read, treat the lock as stale so a corrupt
    // lock cannot permanently wedge the workspace.
    if let Ok(contents) = fs::read_to_string(lock_path) {
        if let Ok(written) =
            chrono::DateTime::parse_from_rfc3339(contents.trim())
        {
            let age = chrono::Utc::now()
                .signed_duration_since(written.with_timezone(&chrono::Utc));
            return age.num_seconds() >= RUNTIME_LOCK_STALE_SECS;
        }
    }
    match fs::metadata(lock_path).and_then(|meta| meta.modified()) {
        Ok(modified) => modified
            .elapsed()
            .map(|age| age.as_secs() as i64 >= RUNTIME_LOCK_STALE_SECS)
            .unwrap_or(true),
        Err(_) => true,
    }
}

struct DefaultTicketStateResolver {
    store: TicketStore,
    workspace_slug: String,
}

impl SessionTicketStateResolver for DefaultTicketStateResolver {
    fn resolve_ticket_state(
        &self,
        ticket_urn: &str,
    ) -> Result<Option<String>, String> {
        let parsed =
            parse_entity_urn(ticket_urn).map_err(|error| error.to_string())?;
        if parsed.kind != SessionPinnedEntityKind::Ticket {
            return Err(format!("not a ticket URN: {ticket_urn}"));
        }
        // The default resolver only queries the sibling `.ticket` store for the
        // session's own workspace. Cross-workspace ticket URNs are rejected
        // explicitly rather than silently resolved against the wrong store.
        if parsed.workspace_slug != self.workspace_slug {
            return Err(format!(
                "unsupported cross-workspace ticket routing: URN workspace `{}` \
                 does not match session workspace `{}` ({ticket_urn})",
                parsed.workspace_slug, self.workspace_slug
            ));
        }
        let ticket_id =
            Uuid::parse_str(&parsed.entity_id).map_err(|error| {
                format!("invalid ticket id in URN {ticket_urn}: {error}")
            })?;
        match self
            .store
            .get_indexed(&ticket_id)
            .map_err(|error| error.to_string())?
        {
            // A resolved ticket may legitimately have no recorded state; keep that
            // distinct from an absent ticket, which is an unavailable-state error.
            Some(indexed) => Ok(indexed.state),
            None => Err(format!("required ticket not found: {ticket_urn}")),
        }
    }
}

fn validation_outcome_label(outcome: ValidationOutcome) -> String {
    match outcome {
        ValidationOutcome::Passed => "passed".to_string(),
        ValidationOutcome::Failed => "failed".to_string(),
        ValidationOutcome::Blocked => "blocked".to_string(),
    }
}

fn node_is_effectively_done(
    node: &SessionWorkflowNode,
    live_state: Option<&Option<String>>,
) -> bool {
    if node.kind == crate::SessionWorkflowNodeKind::Ticket {
        // Ticket-backed nodes derive completion exclusively from authoritative
        // live terminal state. Local `Done` status is display/cache only and can
        // never certify completion. When live state is missing or unavailable
        // (no resolution recorded), the node fails closed and is not done.
        return matches!(
            live_state.and_then(|value| value.as_deref()),
            Some("done") | Some("cancelled")
        );
    }

    // Session-only nodes (action/decision/checkpoint/validation) use local status.
    node.status == SessionWorkflowNodeStatus::Done
}

fn render_handoff_record_terminal(record: &SessionHandoffRecord) -> String {
    let mut lines = Vec::new();
    lines.push(format!("handoff {}", record.handoff_id));
    lines.push(format!(
        "workspace_session_id: {}",
        record.workspace_session_id
    ));
    lines.push(format!("outgoing_run_id: {}", record.outgoing_run_id));
    lines.push(format!("resume: {}", record.resume_command));
    lines.push("workflow:".to_string());
    lines.push(format!("  nodes: {}", record.workflow.workflow.nodes.len()));
    lines.push(format!("  edges: {}", record.workflow.workflow.edges.len()));
    let blocked = record
        .workflow
        .workflow
        .nodes
        .iter()
        .filter(|node| node.status != SessionWorkflowNodeStatus::Done)
        .count();
    lines.push(format!("  not_done_nodes: {}", blocked));
    for pin in &record.pinned_entities {
        lines.push(format!(
            "pin {} {}",
            pin.urn,
            format!("{:?}", pin.kind).to_lowercase()
        ));
    }
    for gate in &record.validation {
        lines.push(format!(
            "validation {} required={} outcome={}",
            gate.validation_spec_id,
            gate.required,
            gate.outcome.as_deref().unwrap_or("-")
        ));
    }
    for diag in &record.workflow.diagnostics {
        lines.push(format!(
            "diag {} {} {}",
            diag.node_id, diag.code, diag.message
        ));
    }
    lines.join("\n")
}

fn sort_workflow_graph(graph: &mut crate::SessionWorkflowGraph) {
    graph
        .nodes
        .sort_by(|left, right| left.node_id.cmp(&right.node_id));
    graph.edges.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| {
                format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind))
            })
    });
}

fn workflow_status_label(status: SessionWorkflowNodeStatus) -> &'static str {
    match status {
        SessionWorkflowNodeStatus::Pending => "pending",
        SessionWorkflowNodeStatus::InProgress => "in-progress",
        SessionWorkflowNodeStatus::Blocked => "blocked",
        SessionWorkflowNodeStatus::Done => "done",
        SessionWorkflowNodeStatus::Deferred => "deferred",
    }
}

fn mermaid_node_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('n');
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

fn escape_mermaid_label(label: &str) -> String {
    label
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStorePlan {
    pub record: SessionRecord,
    pub paths: SessionStorePaths,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<PersistedSessionEvents>,
}

impl SessionStorePlan {
    pub fn manifest(&self) -> PersistedSessionManifest {
        PersistedSessionManifest::from(&self.record)
    }

    pub fn transcript(&self) -> PersistedSessionTranscript {
        PersistedSessionTranscript::from(&self.record)
    }

    pub fn persist(&self) -> Result<SessionStorePaths, SessionError> {
        fs::create_dir_all(&self.paths.session_dir).map_err(|source| {
            SessionError::Io {
                path: self.paths.session_dir.clone(),
                source,
            }
        })?;

        let manifest = merge_manifest(
            read_json_if_exists(&self.paths.manifest_path)?,
            self.manifest(),
        );
        let transcript = merge_transcript(
            read_json_if_exists(&self.paths.transcript_path)?,
            self.transcript(),
        )?;

        let merged_events = merge_events(
            read_json_if_exists(&self.paths.events_path)?,
            self.events.clone(),
            self.record.session_id.clone(),
            self.record.captured_at,
        )?;

        write_json(&self.paths.manifest_path, &manifest)?;
        write_json(&self.paths.transcript_path, &transcript)?;
        if let Some(events) = merged_events {
            write_json(&self.paths.events_path, &events)?;
        }

        Ok(self.paths.clone())
    }
}

#[path = "store_helpers.rs"]
mod store_helpers;
use store_helpers::*;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
