use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use serde::{
    Deserialize,
    Serialize,
};

use crate::{
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
        if self.root.as_os_str().is_empty() {
            return Err(SessionError::EmptyStoreRoot);
        }
        validate_segment(&self.workspace_slug, true)?;
        validate_segment(&record.session_id, false)?;

        let session_dir = self
            .root
            .join("sessions")
            .join(&self.workspace_slug)
            .join(&record.session_id);
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

        write_json(&self.paths.manifest_path, &self.manifest())?;
        write_json(&self.paths.transcript_path, &self.transcript())?;

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
        SessionRole,
        SessionStoreConfig,
    };

    fn sample_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 13, 0, 0)
            .single()
            .unwrap()
    }

    fn sample_request() -> SessionCaptureRequest {
        SessionCaptureRequest::copilot(CopilotHookPayload {
            session_id: "session-abc".to_string(),
            workspace_slug: "context-engine".to_string(),
            captured_at: sample_time(),
            conversation_id: Some("conversation-abc".to_string()),
            agent_id: Some("github-copilot-gpt-5.4".to_string()),
            model: Some("GPT-5.4".to_string()),
            trigger: Some("post-turn".to_string()),
            messages: vec![CopilotHookMessage {
                role: SessionRole::User,
                content: "Persist this chat".to_string(),
                tool_name: None,
                captured_at: None,
            }],
        })
    }

    #[test]
    fn store_plan_uses_workspace_and_session_id_in_paths() {
        let config = SessionStoreConfig::new(".memory-api", "context-engine");
        let plan = config.plan_capture(sample_request()).unwrap();

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
        let mut request = sample_request();
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

        let plan = config.persist_capture(sample_request()).unwrap();
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
    fn persist_capture_overwrites_existing_files_with_latest_record() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

        let mut initial = sample_request();
        initial.payload.messages[0].content = "first".to_string();
        config.persist_capture(initial).unwrap();

        let mut updated = sample_request();
        updated.payload.messages[0].content = "second".to_string();
        let plan = config.persist_capture(updated).unwrap();
        let transcript_text = std::fs::read_to_string(&plan.paths.transcript_path).unwrap();
        let transcript: PersistedSessionTranscript =
            serde_json::from_str(&transcript_text).unwrap();

        assert_eq!(transcript.turns[0].content, "second");
    }
}