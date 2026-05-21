use std::{
    fmt,
    str::FromStr,
};

use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeedbackRating {
    Helpful,
    Mixed,
    NotHelpful,
}

impl FeedbackRating {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Helpful => "helpful",
            Self::Mixed => "mixed",
            Self::NotHelpful => "not-helpful",
        }
    }
}

impl fmt::Display for FeedbackRating {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FeedbackRating {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "helpful" => Ok(Self::Helpful),
            "mixed" => Ok(Self::Mixed),
            "not-helpful" => Ok(Self::NotHelpful),
            other => Err(format!(
                "invalid feedback rating '{other}', expected helpful, mixed, or not-helpful"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeedbackNoteKind {
    Note,
    Suggestion,
}

impl FeedbackNoteKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Suggestion => "suggestion",
        }
    }
}

impl fmt::Display for FeedbackNoteKind {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FeedbackNoteKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "note" => Ok(Self::Note),
            "suggestion" => Ok(Self::Suggestion),
            other => Err(format!(
                "invalid feedback note kind '{other}', expected note or suggestion"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleFeedbackEvent {
    pub timestamp: String,
    pub rating: FeedbackRating,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_kind: Option<FeedbackNoteKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_or_user_id: Option<String>,
}

impl RuleFeedbackEvent {
    pub fn has_note(&self) -> bool {
        self.note_text.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFeedbackInput {
    pub rating: FeedbackRating,
    pub note_text: Option<String>,
    pub note_kind: Option<FeedbackNoteKind>,
    pub session_id: Option<String>,
    pub agent_or_user_id: Option<String>,
}

impl RuleFeedbackInput {
    pub fn new(
        rating: FeedbackRating,
        note_text: Option<String>,
        note_kind: Option<FeedbackNoteKind>,
        session_id: Option<String>,
        agent_or_user_id: Option<String>,
    ) -> Result<Self, String> {
        let note_text = normalize_optional(note_text);
        let session_id = normalize_optional(session_id);
        let agent_or_user_id = normalize_optional(agent_or_user_id);

        let note_kind = match (note_text.as_ref(), note_kind) {
            (Some(_), Some(kind)) => Some(kind),
            (Some(_), None) => Some(FeedbackNoteKind::Note),
            (None, None) => None,
            (None, Some(_)) => {
                return Err("feedback note kind requires feedback note text"
                    .to_string());
            },
        };

        match (session_id.as_ref(), agent_or_user_id.as_ref()) {
            (Some(_), Some(_)) | (None, None) => {},
            _ => {
                return Err(
                    "feedback session references require session_id and agent_or_user_id together"
                        .to_string(),
                );
            },
        }

        Ok(Self {
            rating,
            note_text,
            note_kind,
            session_id,
            agent_or_user_id,
        })
    }

    pub fn into_event(self) -> RuleFeedbackEvent {
        RuleFeedbackEvent {
            timestamp: Utc::now().to_rfc3339(),
            rating: self.rating,
            note_text: self.note_text,
            note_kind: self.note_kind,
            session_id: self.session_id,
            agent_or_user_id: self.agent_or_user_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackSummary {
    pub helpful_count: i64,
    pub mixed_count: i64,
    pub not_helpful_count: i64,
    pub note_count: i64,
    pub unresolved_count: i64,
    pub last_at: Option<String>,
}

impl FeedbackSummary {
    pub fn from_events(events: &[RuleFeedbackEvent]) -> Self {
        let mut summary = Self {
            helpful_count: 0,
            mixed_count: 0,
            not_helpful_count: 0,
            note_count: 0,
            unresolved_count: 0,
            last_at: None,
        };

        for event in events {
            match event.rating {
                FeedbackRating::Helpful => summary.helpful_count += 1,
                FeedbackRating::Mixed => summary.mixed_count += 1,
                FeedbackRating::NotHelpful => {
                    summary.not_helpful_count += 1;
                },
            }

            if event.has_note() {
                summary.note_count += 1;
                summary.unresolved_count += 1;
            }
            summary.last_at = Some(event.timestamp.clone());
        }

        summary
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_input_defaults_note_kind_when_note_text_is_present() {
        let input = RuleFeedbackInput::new(
            FeedbackRating::Mixed,
            Some("  Needs refinement  ".to_string()),
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(input.note_text.as_deref(), Some("Needs refinement"));
        assert_eq!(input.note_kind, Some(FeedbackNoteKind::Note));
    }

    #[test]
    fn feedback_input_rejects_partial_session_reference() {
        let err = RuleFeedbackInput::new(
            FeedbackRating::Helpful,
            None,
            None,
            Some("session-1".to_string()),
            None,
        )
        .unwrap_err();

        assert!(err.contains("session_id and agent_or_user_id together"));
    }
}
