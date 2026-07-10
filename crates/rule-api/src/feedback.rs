use std::{
    collections::HashMap,
    fmt,
    fs,
    io::{
        BufRead,
        BufReader,
        Write,
    },
    path::{
        Path,
        PathBuf,
    },
    str::FromStr,
};

#[path = "feedback_io.rs"]
mod feedback_io;
use chrono::Utc;
use feedback_io::*;
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

/// Classification of the author submitting a feedback or usage event during
/// ingestion. Baseline abuse boundaries key off this classification; full
/// governance is handled by a downstream ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeedbackAuthorKind {
    Human,
    PrivilegedAgent,
}

impl FeedbackAuthorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::PrivilegedAgent => "privileged-agent",
        }
    }
}

impl fmt::Display for FeedbackAuthorKind {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FeedbackAuthorKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "human" => Ok(Self::Human),
            "privileged-agent" | "privileged_agent" | "agent" =>
                Ok(Self::PrivilegedAgent),
            other => Err(format!(
                "invalid feedback author kind '{other}', expected human or privileged-agent"
            )),
        }
    }
}

/// Normalized ingestion author: a classification plus an optional identity.
///
/// Metadata normalization trims the identity and treats empty-after-trim as
/// absent. `privileged-agent` ingestion requires a non-empty identity;
/// `human` ingestion may omit it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestAuthor {
    kind: FeedbackAuthorKind,
    id: Option<String>,
}

impl IngestAuthor {
    pub fn new(
        kind: FeedbackAuthorKind,
        id: Option<String>,
    ) -> Result<Self, String> {
        let id = normalize_optional(id);
        if kind == FeedbackAuthorKind::PrivilegedAgent && id.is_none() {
            return Err(
                "privileged-agent ingestion requires a non-empty agent_or_user_id"
                    .to_string(),
            );
        }
        Ok(Self { kind, id })
    }

    pub fn human(id: Option<String>) -> Result<Self, String> {
        Self::new(FeedbackAuthorKind::Human, id)
    }

    pub fn privileged_agent(id: impl Into<String>) -> Result<Self, String> {
        Self::new(FeedbackAuthorKind::PrivilegedAgent, Some(id.into()))
    }

    pub fn kind(&self) -> FeedbackAuthorKind {
        self.kind
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
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

        let note_kind = resolve_note_kind(note_text.as_deref(), note_kind)?;

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityUrn {
    workspace: String,
    store: String,
    entity: String,
}

impl EntityUrn {
    pub fn new(
        workspace: impl Into<String>,
        store: impl Into<String>,
        entity: impl Into<String>,
    ) -> Result<Self, String> {
        let workspace = normalize_required(workspace.into(), "workspace")?;
        let store = normalize_required(store.into(), "store")?;
        let entity = normalize_required(entity.into(), "entity")?;

        Ok(Self {
            workspace,
            store,
            entity,
        })
    }

    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    pub fn store(&self) -> &str {
        &self.store
    }

    pub fn entity(&self) -> &str {
        &self.entity
    }

    pub fn as_str(&self) -> String {
        format!("ce://{}/{}/{}", self.workspace, self.store, self.entity)
    }

    pub fn rule(
        workspace: impl Into<String>,
        entity: impl Into<String>,
    ) -> Result<Self, String> {
        Self::new(workspace, "rule", entity)
    }

    pub fn spec(
        workspace: impl Into<String>,
        entity: impl Into<String>,
    ) -> Result<Self, String> {
        Self::new(workspace, "spec", entity)
    }

    pub fn ticket(
        workspace: impl Into<String>,
        entity: impl Into<String>,
    ) -> Result<Self, String> {
        Self::new(workspace, "ticket", entity)
    }
}

impl fmt::Display for EntityUrn {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

impl FromStr for EntityUrn {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        let Some(rest) = trimmed.strip_prefix("ce://") else {
            return Err(format!(
                "invalid entity urn '{trimmed}', expected ce://<workspace>/<store>/<entity>"
            ));
        };
        let mut parts = rest.split('/');
        let workspace = parts.next().unwrap_or_default();
        let store = parts.next().unwrap_or_default();
        let entity = parts.next().unwrap_or_default();
        if parts.next().is_some() {
            return Err(format!(
                "invalid entity urn '{trimmed}', expected exactly 3 path segments"
            ));
        }

        Self::new(workspace, store, entity)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUsageEvent {
    pub timestamp: String,
    pub urn: EntityUrn,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_kind: Option<FeedbackAuthorKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
}

impl EntityUsageEvent {
    pub fn new(urn: EntityUrn) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            urn,
            author_kind: None,
            author_id: None,
        }
    }

    pub fn with_author(
        urn: EntityUrn,
        author: &IngestAuthor,
    ) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            urn,
            author_kind: Some(author.kind()),
            author_id: author.id().map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRatingEvent {
    pub timestamp: String,
    pub urn: EntityUrn,
    pub rating: FeedbackRating,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_kind: Option<FeedbackNoteKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_or_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_kind: Option<FeedbackAuthorKind>,
}

impl EntityRatingEvent {
    pub fn has_note(&self) -> bool {
        self.note_text.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRatingInput {
    pub rating: FeedbackRating,
    pub note_text: Option<String>,
    pub note_kind: Option<FeedbackNoteKind>,
    pub session_id: Option<String>,
    pub agent_or_user_id: Option<String>,
}

impl EntityRatingInput {
    pub fn new(
        rating: FeedbackRating,
        note_text: Option<String>,
        note_kind: Option<FeedbackNoteKind>,
        session_id: Option<String>,
        agent_or_user_id: Option<String>,
    ) -> Result<Self, String> {
        let input = RuleFeedbackInput::new(
            rating,
            note_text,
            note_kind,
            session_id,
            agent_or_user_id,
        )?;

        Ok(Self {
            rating: input.rating,
            note_text: input.note_text,
            note_kind: input.note_kind,
            session_id: input.session_id,
            agent_or_user_id: input.agent_or_user_id,
        })
    }

    pub fn into_event(
        self,
        urn: EntityUrn,
    ) -> EntityRatingEvent {
        EntityRatingEvent {
            timestamp: Utc::now().to_rfc3339(),
            urn,
            rating: self.rating,
            note_text: self.note_text,
            note_kind: self.note_kind,
            session_id: self.session_id,
            agent_or_user_id: self.agent_or_user_id,
            author_kind: None,
        }
    }

    /// Build a rating event tagged with an ingestion author. The paired
    /// session/agent invariant is enforced when the [`EntityRatingInput`] is
    /// constructed; the author identity is applied earlier (during ingestion)
    /// so callers may submit a `session_id` without redundantly repeating the
    /// author id. This only stamps the author classification onto the event.
    pub fn into_event_with_author(
        self,
        urn: EntityUrn,
        author: &IngestAuthor,
    ) -> EntityRatingEvent {
        EntityRatingEvent {
            timestamp: Utc::now().to_rfc3339(),
            urn,
            rating: self.rating,
            note_text: self.note_text,
            note_kind: self.note_kind,
            session_id: self.session_id,
            agent_or_user_id: self.agent_or_user_id,
            author_kind: Some(author.kind()),
        }
    }
}

/// Raw, unvalidated rating submission for author-attributed ingestion.
///
/// Validation — including the paired `session_id`/`agent_or_user_id`
/// invariant — runs inside [`EntityFeedbackStore::ingest_rating`] *after* the
/// author identity has backfilled a missing `agent_or_user_id`. This lets a
/// caller submit a `session_id` alongside an authenticated [`IngestAuthor`]
/// without redundantly copying the author id into the submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRatingSubmission {
    pub rating: FeedbackRating,
    pub note_text: Option<String>,
    pub note_kind: Option<FeedbackNoteKind>,
    pub session_id: Option<String>,
    pub agent_or_user_id: Option<String>,
}

impl EntityRatingSubmission {
    pub fn new(rating: FeedbackRating) -> Self {
        Self {
            rating,
            note_text: None,
            note_kind: None,
            session_id: None,
            agent_or_user_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityFeedbackSummary {
    pub urn: EntityUrn,
    pub usage_count: i64,
    pub helpful_count: i64,
    pub mixed_count: i64,
    pub not_helpful_count: i64,
    pub note_count: i64,
    pub unresolved_count: i64,
    pub last_used_at: Option<String>,
    pub last_rated_at: Option<String>,
}

impl EntityFeedbackSummary {
    fn new(urn: EntityUrn) -> Self {
        Self {
            urn,
            usage_count: 0,
            helpful_count: 0,
            mixed_count: 0,
            not_helpful_count: 0,
            note_count: 0,
            unresolved_count: 0,
            last_used_at: None,
            last_rated_at: None,
        }
    }

    pub fn has_low_rating(&self) -> bool {
        self.not_helpful_count > 0
            && self.not_helpful_count >= self.helpful_count
    }

    pub fn has_unresolved_notes(&self) -> bool {
        self.unresolved_count > 0
    }
}

#[derive(Debug, Default, Clone)]
pub struct EntityFeedbackCore {
    usage_events: Vec<EntityUsageEvent>,
    rating_events: Vec<EntityRatingEvent>,
}
const FEEDBACK_CORE_DIR: &str = "feedback-core";
const FEEDBACK_CORE_USAGE_FILE: &str = "usage-events.ndjson";
const FEEDBACK_CORE_RATING_FILE: &str = "rating-events.ndjson";

/// Baseline retention policy for the persisted feedback logs. A `None` bound
/// leaves that dimension unconstrained. `max_events` is applied per event kind
/// (usage / rating) after the age filter, keeping the most recent events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub max_age: Option<chrono::Duration>,
    pub max_events: Option<usize>,
}

impl RetentionPolicy {
    pub fn max_age_days(days: i64) -> Self {
        Self {
            max_age: Some(chrono::Duration::days(days)),
            max_events: None,
        }
    }

    pub fn max_events(count: usize) -> Self {
        Self {
            max_age: None,
            max_events: Some(count),
        }
    }
}

/// Retained/removed counts for a single event kind after applying retention.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionKindOutcome {
    pub retained: usize,
    pub removed: usize,
}

/// Combined retention outcome across both persisted event kinds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionOutcome {
    pub usage: RetentionKindOutcome,
    pub rating: RetentionKindOutcome,
}

impl RetentionOutcome {
    pub fn total_removed(&self) -> usize {
        self.usage.removed + self.rating.removed
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityFeedbackStore {
    root: PathBuf,
    workspace_slug: String,
}

#[path = "feedback_store.rs"]
mod feedback_store;

#[cfg(test)]
#[path = "feedback_tests.rs"]
mod tests;

fn normalize_required(
    value: String,
    field: &str,
) -> Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(format!("entity urn {field} segment cannot be empty"));
    }
    if normalized.contains('/') {
        return Err(format!(
            "entity urn {field} segment cannot contain '/': {normalized}"
        ));
    }
    Ok(normalized.to_string())
}
