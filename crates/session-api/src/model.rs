use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionLinks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ticket_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spec_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doc_evidence_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log_ids: Vec<String>,
}

impl SessionLinks {
    pub fn links_to_ticket(
        &self,
        ticket_id: &str,
    ) -> bool {
        self.ticket_ids.iter().any(|id| id == ticket_id)
    }

    pub fn links_to_spec(
        &self,
        spec_id: &str,
    ) -> bool {
        self.spec_ids.iter().any(|id| id == spec_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTurn {
    pub sequence: usize,
    pub role: SessionRole,
    pub content: String,
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub workspace_slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub source: String,
    pub started_at: DateTime<Utc>,
    pub captured_at: DateTime<Utc>,
    pub metadata: SessionMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<SessionTurn>,
    #[serde(default)]
    pub links: SessionLinks,
}

impl SessionRecord {
    pub fn has_turns(&self) -> bool {
        !self.turns.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    use super::{
        SessionLinks,
        SessionMetadata,
        SessionRecord,
        SessionRole,
        SessionTurn,
    };

    fn sample_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 12, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn session_record_round_trips_through_serde() {
        let record = SessionRecord {
            session_id: "session-123".to_string(),
            source: "copilot-hook".to_string(),
            started_at: sample_time(),
            captured_at: sample_time(),
            metadata: SessionMetadata {
                workspace_slug: "context-engine".to_string(),
                conversation_id: Some("conversation-1".to_string()),
                agent_id: Some("github-copilot-gpt-5.4".to_string()),
                model: Some("GPT-5.4".to_string()),
                trigger: Some("post-turn".to_string()),
            },
            turns: vec![SessionTurn {
                sequence: 0,
                role: SessionRole::User,
                content: "Summarize the test failures".to_string(),
                captured_at: sample_time(),
                tool_name: None,
            }],
            links: SessionLinks {
                ticket_ids: vec!["ticket-1".to_string()],
                spec_ids: vec!["spec-1".to_string()],
                doc_evidence_ids: vec!["doc-1".to_string()],
                log_ids: vec!["log-1".to_string()],
            },
        };

        let json = serde_json::to_string_pretty(&record).unwrap();
        let reparsed: SessionRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(reparsed, record);
        assert!(record.links.links_to_ticket("ticket-1"));
        assert!(record.links.links_to_spec("spec-1"));
    }
}