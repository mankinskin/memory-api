use std::{
    fs,
    io::ErrorKind,
    path::{
        Path,
        PathBuf,
    },
};

use serde::{
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};

use crate::{
    hook::copilot_payload_from_transcript_path,
    CopilotHookPayload,
    SessionCaptureRequest,
    SessionError,
    SessionLinks,
    SessionMetadata,
    SessionRecord,
    SessionTurn,
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
        let manifest: PersistedSessionManifest = read_json(&paths.manifest_path)?;
        let transcript: PersistedSessionTranscript = read_json(&paths.transcript_path)?;

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
        for entry in fs::read_dir(&sessions_root).map_err(|source| SessionError::Io {
            path: sessions_root.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?;
            let file_type = entry.file_type().map_err(|source| SessionError::Io {
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

    fn paths_for_session_id(
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

        if manifest_path.parent().is_none() || transcript_path.parent().is_none() {
            return Err(SessionError::InvalidStorePath(session_dir));
        }

        Ok(SessionStorePaths {
            session_dir,
            manifest_path,
            transcript_path,
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
        let record = request.into_record()?;
        let paths = self.paths_for(&record)?;
        Ok(SessionStorePlan { record, paths })
    }

    pub fn persist_capture(
        &self,
        request: SessionCaptureRequest,
    ) -> Result<SessionStorePlan, SessionError> {
        let plan = self.plan_capture(request)?;
        plan.persist()?;
        Ok(plan)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStorePaths {
    pub session_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub transcript_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStorePlan {
    pub record: SessionRecord,
    pub paths: SessionStorePaths,
}

impl SessionStorePlan {
    pub fn manifest(&self) -> PersistedSessionManifest {
        PersistedSessionManifest::from(&self.record)
    }

    pub fn transcript(&self) -> PersistedSessionTranscript {
        PersistedSessionTranscript::from(&self.record)
    }

    pub fn persist(&self) -> Result<SessionStorePaths, SessionError> {
        fs::create_dir_all(&self.paths.session_dir).map_err(|source| SessionError::Io {
            path: self.paths.session_dir.clone(),
            source,
        })?;

        let manifest = merge_manifest(
            read_json_if_exists(&self.paths.manifest_path)?,
            self.manifest(),
        );
        let transcript = merge_transcript(
            read_json_if_exists(&self.paths.transcript_path)?,
            self.transcript(),
        )?;

        write_json(&self.paths.manifest_path, &manifest)?;
        write_json(&self.paths.transcript_path, &transcript)?;

        Ok(self.paths.clone())
    }
}

fn write_json<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), SessionError> {
    let encoded = serde_json::to_vec_pretty(value).map_err(|source| SessionError::Serialize {
        path: path.to_path_buf(),
        source,
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
    serde_json::from_slice(&encoded).map_err(|source| SessionError::Deserialize {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json_if_exists<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, SessionError> {
    match fs::read(path) {
        Ok(encoded) => serde_json::from_slice(&encoded)
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
        incoming.metadata = merge_metadata(existing.metadata, incoming.metadata);
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
        model: incoming.model.or(existing.model),
        trigger: incoming.trigger.or(existing.trigger),
    }
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

            if shared_prefix_len < existing.turns.len() && shared_prefix_len < incoming.turns.len() {
                return Err(SessionError::TranscriptConflict {
                    session_id: existing.session_id,
                    existing_turns: existing.turns.len(),
                    incoming_turns: incoming.turns.len(),
                });
            }

            if incoming.turns.len() > existing.turns.len() {
                existing
                    .turns
                    .extend(incoming.turns.into_iter().skip(existing.turns.len()));
            }

            if incoming.captured_at > existing.captured_at {
                existing.captured_at = incoming.captured_at;
            }

            Ok(existing)
        }
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
        if record.metadata.conversation_id.as_deref() != Some(conversation_id.as_str()) {
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
    if value.trim().is_empty() || value.chars().any(|ch| invalid.contains(&ch)) {
        return if is_workspace_slug {
            Err(SessionError::InvalidWorkspaceSlug(value.to_string()))
        } else {
            Err(SessionError::InvalidSessionId(value.to_string()))
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use crate::{
        CopilotHookMessage,
        CopilotHookPayload,
        PersistedSessionManifest,
        PersistedSessionTranscript,
        SessionCaptureRequest,
        SessionError,
        SessionQuery,
        SessionRole,
        SessionStoreConfig,
    };

    fn sample_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 13, 0, 0)
            .single()
            .unwrap()
    }

    fn sample_time_later() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 13, 5, 0)
            .single()
            .unwrap()
    }

    fn sample_payload(
        session_id: &str,
        conversation_id: Option<&str>,
        captured_at: chrono::DateTime<chrono::Utc>,
        messages: &[&str],
    ) -> CopilotHookPayload {
        CopilotHookPayload {
            session_id: session_id.to_string(),
            workspace_slug: "context-engine".to_string(),
            captured_at,
            conversation_id: conversation_id.map(str::to_string),
            agent_id: Some("github-copilot-gpt-5.4".to_string()),
            model: Some("GPT-5.4".to_string()),
            trigger: Some("post-turn".to_string()),
            messages: messages
                .iter()
                .enumerate()
                .map(|(index, content)| CopilotHookMessage {
                    role: if index % 2 == 0 {
                        SessionRole::User
                    } else {
                        SessionRole::Assistant
                    },
                    content: (*content).to_string(),
                    tool_name: None,
                    captured_at: None,
                })
                .collect(),
        }
    }

    fn sample_request(
        session_id: &str,
        conversation_id: Option<&str>,
        captured_at: chrono::DateTime<chrono::Utc>,
        messages: &[&str],
    ) -> SessionCaptureRequest {
        SessionCaptureRequest::copilot(sample_payload(
            session_id,
            conversation_id,
            captured_at,
            messages,
        ))
    }

    #[test]
    fn store_plan_uses_workspace_and_session_id_in_paths() {
        let config = SessionStoreConfig::new(".memory-api", "context-engine");
        let plan = config
            .plan_capture(sample_request(
                "session-abc",
                Some("conversation-abc"),
                sample_time(),
                &["Persist this chat"],
            ))
            .unwrap();

        assert_eq!(
            plan.paths.manifest_path,
            std::path::PathBuf::from(
                ".memory-api/sessions/context-engine/session-abc/session.json"
            )
        );
        assert_eq!(
            plan.paths.transcript_path,
            std::path::PathBuf::from(
                ".memory-api/sessions/context-engine/session-abc/transcript.json"
            )
        );
    }

    #[test]
    fn store_plan_rejects_invalid_path_segments() {
        let config = SessionStoreConfig::new(".memory-api", "context-engine");
        let mut request = sample_request(
            "session-abc",
            Some("conversation-abc"),
            sample_time(),
            &["Persist this chat"],
        );
        request.payload.session_id = "session/abc".to_string();

        let error = config.plan_capture(request).unwrap_err();

        assert!(matches!(
            error,
            SessionError::InvalidSessionId(ref value) if value == "session/abc"
        ));
    }

    #[test]
    fn persist_capture_writes_manifest_and_transcript_files() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

        let plan = config
            .persist_capture(sample_request(
                "session-abc",
                Some("conversation-abc"),
                sample_time(),
                &["Persist this chat"],
            ))
            .unwrap();
        let manifest_text = std::fs::read_to_string(&plan.paths.manifest_path).unwrap();
        let transcript_text = std::fs::read_to_string(&plan.paths.transcript_path).unwrap();

        let manifest: PersistedSessionManifest = serde_json::from_str(&manifest_text).unwrap();
        let transcript: PersistedSessionTranscript =
            serde_json::from_str(&transcript_text).unwrap();

        assert_eq!(manifest.session_id, "session-abc");
        assert_eq!(manifest.metadata.workspace_slug, "context-engine");
        assert_eq!(transcript.session_id, "session-abc");
        assert_eq!(transcript.turns.len(), 1);
        assert_eq!(transcript.turns[0].content, "Persist this chat");
    }

    #[test]
    fn persist_capture_appends_only_new_turns_from_later_capture() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

        config
            .persist_capture(sample_request(
                "session-abc",
                Some("conversation-abc"),
                sample_time(),
                &["first"],
            ))
            .unwrap();

        let plan = config
            .persist_capture(sample_request(
                "session-abc",
                Some("conversation-abc"),
                sample_time_later(),
                &["first", "second"],
            ))
            .unwrap();
        config
            .persist_capture(sample_request(
                "session-abc",
                Some("conversation-abc"),
                sample_time_later(),
                &["first", "second"],
            ))
            .unwrap();
        let transcript_text = std::fs::read_to_string(&plan.paths.transcript_path).unwrap();
        let transcript: PersistedSessionTranscript =
            serde_json::from_str(&transcript_text).unwrap();

        assert_eq!(transcript.turns.len(), 2);
        assert_eq!(transcript.turns[0].content, "first");
        assert_eq!(transcript.turns[0].captured_at, sample_time());
        assert_eq!(transcript.turns[1].content, "second");
        assert_eq!(transcript.turns[1].captured_at, sample_time_later());
    }

    #[test]
    fn read_session_reconstructs_persisted_record() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

        config
            .persist_capture(sample_request(
                "session-abc",
                Some("conversation-abc"),
                sample_time(),
                &["first"],
            ))
            .unwrap();
        config
            .persist_capture(sample_request(
                "session-abc",
                Some("conversation-abc"),
                sample_time_later(),
                &["first", "second"],
            ))
            .unwrap();

        let record = config.read_session("session-abc").unwrap();

        assert_eq!(record.session_id, "session-abc");
        assert_eq!(record.started_at, sample_time());
        assert_eq!(record.captured_at, sample_time_later());
        assert_eq!(record.turns.len(), 2);
        assert_eq!(record.turns[0].content, "first");
        assert_eq!(record.turns[1].content, "second");
    }

    #[test]
    fn capture_copilot_hook_persists_payload() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

        let plan = config
            .capture_copilot_hook(sample_payload(
                "session-hook",
                Some("conversation-hook"),
                sample_time(),
                &["Persist from hook"],
            ))
            .unwrap();
        let record = config.read_session("session-hook").unwrap();

        assert!(plan.paths.manifest_path.exists());
        assert_eq!(record.session_id, "session-hook");
        assert_eq!(record.turns.len(), 1);
        assert_eq!(record.turns[0].content, "Persist from hook");
    }

    #[test]
    fn query_sessions_filters_by_text_and_metadata() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

        config
            .capture_copilot_hook(sample_payload(
                "session-alpha",
                Some("conversation-alpha"),
                sample_time(),
                &["Investigate failing test"],
            ))
            .unwrap();
        config
            .capture_copilot_hook(sample_payload(
                "session-beta",
                Some("conversation-beta"),
                sample_time_later(),
                &["Document hook query behavior"],
            ))
            .unwrap();

        let by_text = config
            .query_sessions(&SessionQuery {
                text: Some("hook query".to_string()),
                ..SessionQuery::default()
            })
            .unwrap();
        let by_conversation = config
            .query_sessions(&SessionQuery {
                conversation_id: Some("conversation-alpha".to_string()),
                ..SessionQuery::default()
            })
            .unwrap();

        assert_eq!(by_text.len(), 1);
        assert_eq!(by_text[0].session_id, "session-beta");
        assert_eq!(by_conversation.len(), 1);
        assert_eq!(by_conversation[0].session_id, "session-alpha");
    }

    #[test]
    fn capture_copilot_transcript_persists_visible_transcript_messages() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
        let transcript_path = tempdir.path().join("copilot.jsonl");

        std::fs::write(
            &transcript_path,
            concat!(
                "{\"type\":\"session.start\",\"timestamp\":\"2026-06-02T23:06:54.049Z\",\"data\":{\"sessionId\":\"session-transcript\",\"producer\":\"copilot-agent\",\"startTime\":\"2026-06-02T23:06:54.049Z\"}}\n",
                "{\"type\":\"user.message\",\"timestamp\":\"2026-06-02T23:07:00.000Z\",\"data\":{\"content\":\"Persist this transcript\"}}\n",
                "{\"type\":\"assistant.message\",\"timestamp\":\"2026-06-02T23:07:05.000Z\",\"data\":{\"content\":\"Transcript persisted.\"}}\n",
                "{\"type\":\"assistant.message\",\"timestamp\":\"2026-06-02T23:07:06.000Z\",\"data\":{\"content\":\"\"}}\n"
            ),
        )
        .unwrap();

        let plan = config
            .capture_copilot_transcript(&transcript_path, "stop")
            .unwrap();
        let record = config.read_session("session-transcript").unwrap();

        assert!(plan.paths.manifest_path.exists());
        assert_eq!(record.session_id, "session-transcript");
        assert_eq!(record.metadata.trigger.as_deref(), Some("stop"));
        assert_eq!(record.turns.len(), 2);
        assert_eq!(record.turns[0].content, "Persist this transcript");
        assert_eq!(record.turns[1].content, "Transcript persisted.");
    }
}