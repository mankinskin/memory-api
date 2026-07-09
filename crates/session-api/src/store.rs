use std::{
    fs,
    io::ErrorKind,
    path::{
        Path,
        PathBuf,
    },
};

use serde::{
    Deserialize,
    Serialize,
    de::DeserializeOwned,
};

use crate::{
    SessionAuditReport,
    SessionAuditSelector,
    CopilotHookPayload,
    SessionCaptureRequest,
    SessionError,
    SessionLinks,
    SessionMetadata,
    SessionRecord,
    SessionTurn,
    SessionWorktreeAllocationMode,
    SessionWorktreeAssignment,
    SessionWorktreeStatus,
    SESSION_SCHEMA_VERSION,
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
                Some(current) => {
                    record.captured_at > current.captured_at
                        || (record.captured_at == current.captured_at
                            && record.session_id < current.session_id)
                },
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
