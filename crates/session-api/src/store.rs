use std::path::PathBuf;

use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    SessionCaptureRequest,
    SessionError,
    SessionRecord,
};

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

    use crate::{
        CopilotHookMessage,
        CopilotHookPayload,
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

        assert_eq!(error, SessionError::InvalidSessionId("session/abc".to_string()));
    }
}