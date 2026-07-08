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
    hook::{
        CopilotHookEvent,
        copilot_payload_from_transcript_path,
    },
    peek::{
        SessionSkeleton,
        SessionTurnRange,
        peek_skeleton,
        peek_turn_range,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSessionManifest {
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
    pub session_id: String,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<SessionTurn>,
}

impl From<&SessionRecord> for PersistedSessionTranscript {
    fn from(record: &SessionRecord) -> Self {
        Self {
            session_id: record.session_id.clone(),
            captured_at: record.captured_at,
            turns: record.turns.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSessionEvents {
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
        let transcript: PersistedSessionTranscript =
            read_json(&paths.transcript_path)?;

        Ok(SessionRecord {
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

    pub(crate) fn paths_for_session_id(
        &self,
        session_id: &str,
    ) -> Result<SessionStorePaths, SessionError> {
        if self.root.as_os_str().is_empty() {
            return Err(SessionError::EmptyStoreRoot);
        }
        validate_segment(&self.workspace_slug, true)?;
        validate_segment(session_id, false)?;

        let session_dir = self
            .root
            .join("sessions")
            .join(&self.workspace_slug)
            .join(session_id);
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
        validate_segment(&self.workspace_slug, true)?;
        Ok(self.root.join("sessions").join(&self.workspace_slug))
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

fn write_json<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), SessionError> {
    let encoded = serde_json::to_vec_pretty(value).map_err(|source| {
        SessionError::Serialize {
            path: path.to_path_buf(),
            source,
        }
    })?;
    fs::write(path, encoded).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, SessionError> {
    let encoded = fs::read(path).map_err(|source| match source.kind() {
        ErrorKind::NotFound => SessionError::NotFound {
            path: path.to_path_buf(),
        },
        _ => SessionError::Io {
            path: path.to_path_buf(),
            source,
        },
    })?;
    serde_json::from_slice(&encoded).map_err(|source| {
        SessionError::Deserialize {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn read_json_if_exists<T: DeserializeOwned>(
    path: &Path
) -> Result<Option<T>, SessionError> {
    match fs::read(path) {
        Ok(encoded) =>
            serde_json::from_slice(&encoded)
                .map(Some)
                .map_err(|source| SessionError::Deserialize {
                    path: path.to_path_buf(),
                    source,
                }),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(SessionError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn merge_manifest(
    existing: Option<PersistedSessionManifest>,
    mut incoming: PersistedSessionManifest,
) -> PersistedSessionManifest {
    if let Some(existing) = existing {
        if existing.started_at < incoming.started_at {
            incoming.started_at = existing.started_at;
        }
        if existing.captured_at > incoming.captured_at {
            incoming.captured_at = existing.captured_at;
        }
        incoming.metadata =
            merge_metadata(existing.metadata, incoming.metadata);
        incoming.links = merge_links(existing.links, incoming.links);
    }

    incoming
}

fn merge_metadata(
    existing: SessionMetadata,
    incoming: SessionMetadata,
) -> SessionMetadata {
    SessionMetadata {
        workspace_slug: if incoming.workspace_slug.trim().is_empty() {
            existing.workspace_slug
        } else {
            incoming.workspace_slug
        },
        conversation_id: incoming.conversation_id.or(existing.conversation_id),
        agent_id: incoming.agent_id.or(existing.agent_id),
        ticket_id: incoming.ticket_id.or(existing.ticket_id),
        model: incoming.model.or(existing.model),
        trigger: incoming.trigger.or(existing.trigger),
        producer: incoming.producer.or(existing.producer),
        copilot_version: incoming.copilot_version.or(existing.copilot_version),
        vscode_version: incoming.vscode_version.or(existing.vscode_version),
        protocol_version: incoming
            .protocol_version
            .or(existing.protocol_version),
        worktree: incoming.worktree.or(existing.worktree),
    }
}

fn validate_worktree_request(
    request: &SessionWorktreeCheckInRequest
) -> Result<(), SessionError> {
    validate_segment(&request.session_id, false)?;
    if request.owner_id.trim().is_empty() {
        return Err(SessionError::MissingOwnerId);
    }
    if request.ticket_id.trim().is_empty() {
        return Err(SessionError::MissingTicketId);
    }
    if request.worktree_path.as_os_str().is_empty() {
        return Err(SessionError::EmptyWorktreePath);
    }
    if request.branch.trim().is_empty() {
        return Err(SessionError::EmptyWorktreeBranch);
    }
    Ok(())
}

fn can_reuse_assignment(
    existing: &SessionWorktreeAssignment,
    request: &SessionWorktreeCheckInRequest,
) -> bool {
    existing.status == SessionWorktreeStatus::Active
        && existing.path == request.worktree_path
        && existing.branch == request.branch
        && existing.path.exists()
}

fn receipt_from_record(
    record: &SessionRecord
) -> Result<SessionWorktreeCheckInReceipt, SessionError> {
    let worktree = record.metadata.worktree.clone().ok_or_else(|| {
        SessionError::MissingWorktreeAssignment {
            session_id: record.session_id.clone(),
        }
    })?;

    Ok(SessionWorktreeCheckInReceipt {
        session_id: record.session_id.clone(),
        owner_id: record.metadata.agent_id.clone().unwrap_or_default(),
        ticket_id: record.metadata.ticket_id.clone().unwrap_or_default(),
        worktree_path: worktree.path,
        branch: worktree.branch,
        allocation_mode: worktree.allocation_mode,
        status: worktree.status,
        predecessor_session_id: worktree.predecessor_session_id,
        predecessor_path: worktree.predecessor_path,
    })
}

fn merge_links(
    existing: SessionLinks,
    incoming: SessionLinks,
) -> SessionLinks {
    let mut merged = existing;
    extend_unique(&mut merged.ticket_ids, incoming.ticket_ids);
    extend_unique(&mut merged.spec_ids, incoming.spec_ids);
    extend_unique(&mut merged.doc_evidence_ids, incoming.doc_evidence_ids);
    extend_unique(&mut merged.log_ids, incoming.log_ids);
    merged
}

fn extend_unique(
    target: &mut Vec<String>,
    incoming: Vec<String>,
) {
    for value in incoming {
        if !target.iter().any(|existing| existing == &value) {
            target.push(value);
        }
    }
}

fn merge_events(
    existing: Option<PersistedSessionEvents>,
    incoming: Option<PersistedSessionEvents>,
    session_id: String,
    captured_at: chrono::DateTime<chrono::Utc>,
) -> Result<Option<PersistedSessionEvents>, SessionError> {
    match (existing, incoming) {
        (None, None) => Ok(None),
        (Some(existing), None) => Ok(Some(existing)),
        (None, Some(incoming)) => Ok(Some(incoming)),
        (Some(mut existing), Some(incoming)) => {
            if existing.session_id != incoming.session_id {
                return Err(SessionError::TranscriptConflict {
                    session_id: incoming.session_id,
                    existing_turns: existing.events.len(),
                    incoming_turns: incoming.events.len(),
                });
            }

            let mut known = std::collections::BTreeSet::new();
            for event in &existing.events {
                known.insert(captured_event_key(event));
            }
            for event in incoming.events {
                let key = captured_event_key(&event);
                if known.insert(key) {
                    existing.events.push(event);
                }
            }

            existing.session_id = session_id;
            if captured_at > existing.captured_at {
                existing.captured_at = captured_at;
            }

            Ok(Some(existing))
        },
    }
}

fn captured_event_key(event: &CopilotHookEvent) -> String {
    if let Some(id) = &event.event_id {
        return format!("id:{id}");
    }

    format!(
        "type:{}|ts:{}|msg:{}|call:{}|turn:{}|tool:{}|ok:{}|reason:{}|req:{}|args:{}|data:{}|raw:{}",
        event.event_type.as_deref().unwrap_or(""),
        event
            .captured_at
            .map(|ts| ts.to_rfc3339())
            .unwrap_or_default(),
        event.message_id.as_deref().unwrap_or(""),
        event.tool_call_id.as_deref().unwrap_or(""),
        event.turn_id.as_deref().unwrap_or(""),
        event.tool_name.as_deref().unwrap_or(""),
        event
            .tool_success
            .map(|ok| ok.to_string())
            .unwrap_or_default(),
        event.reasoning_text.as_deref().unwrap_or(""),
        event.tool_requests_json.as_deref().unwrap_or(""),
        event.tool_arguments_json.as_deref().unwrap_or(""),
        event.data_json.as_deref().unwrap_or(""),
        event.raw_event_json.as_deref().unwrap_or(""),
    )
}

fn merge_transcript(
    existing: Option<PersistedSessionTranscript>,
    incoming: PersistedSessionTranscript,
) -> Result<PersistedSessionTranscript, SessionError> {
    match existing {
        None => Ok(incoming),
        Some(mut existing) => {
            if existing.session_id != incoming.session_id {
                return Err(SessionError::TranscriptConflict {
                    session_id: incoming.session_id,
                    existing_turns: existing.turns.len(),
                    incoming_turns: incoming.turns.len(),
                });
            }

            let shared_prefix_len = existing
                .turns
                .iter()
                .zip(&incoming.turns)
                .take_while(|(left, right)| turns_match(left, right))
                .count();

            if shared_prefix_len < existing.turns.len()
                && shared_prefix_len < incoming.turns.len()
            {
                return Err(SessionError::TranscriptConflict {
                    session_id: existing.session_id,
                    existing_turns: existing.turns.len(),
                    incoming_turns: incoming.turns.len(),
                });
            }

            if incoming.turns.len() > existing.turns.len() {
                existing.turns.extend(
                    incoming.turns.into_iter().skip(existing.turns.len()),
                );
            }

            if incoming.captured_at > existing.captured_at {
                existing.captured_at = incoming.captured_at;
            }

            Ok(existing)
        },
    }
}

fn turns_match(
    left: &SessionTurn,
    right: &SessionTurn,
) -> bool {
    left.sequence == right.sequence
        && left.role == right.role
        && left.content == right.content
        && left.tool_name == right.tool_name
        && left.event_meta == right.event_meta
}

fn session_matches_query(
    record: &SessionRecord,
    query: &SessionQuery,
) -> bool {
    if let Some(prefix) = &query.session_id_prefix {
        if !record.session_id.starts_with(prefix) {
            return false;
        }
    }

    if let Some(conversation_id) = &query.conversation_id {
        if record.metadata.conversation_id.as_deref()
            != Some(conversation_id.as_str())
        {
            return false;
        }
    }

    if let Some(agent_id) = &query.agent_id {
        if record.metadata.agent_id.as_deref() != Some(agent_id.as_str()) {
            return false;
        }
    }

    if let Some(text) = &query.text {
        let needle = text.to_ascii_lowercase();
        if !record
            .turns
            .iter()
            .any(|turn| turn.content.to_ascii_lowercase().contains(&needle))
        {
            return false;
        }
    }

    true
}

fn validate_segment(
    value: &str,
    is_workspace_slug: bool,
) -> Result<(), SessionError> {
    let invalid = ['/', '\\', ':'];
    if value.trim().is_empty() || value.chars().any(|ch| invalid.contains(&ch))
    {
        return if is_workspace_slug {
            Err(SessionError::InvalidWorkspaceSlug(value.to_string()))
        } else {
            Err(SessionError::InvalidSessionId(value.to_string()))
        };
    }
    Ok(())
}


#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
