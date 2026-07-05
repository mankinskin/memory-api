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
            "privileged-agent" | "privileged_agent" | "agent" => {
                Ok(Self::PrivilegedAgent)
            },
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

impl EntityFeedbackStore {
    pub fn new(
        root: impl Into<PathBuf>,
        workspace_slug: impl Into<String>,
    ) -> Result<Self, String> {
        let workspace_slug = normalize_required(
            workspace_slug.into(),
            "workspace_slug",
        )?;
        Ok(Self {
            root: root.into(),
            workspace_slug,
        })
    }

    pub fn workspace_slug(&self) -> &str {
        &self.workspace_slug
    }

    pub fn record_usage(
        &self,
        urn: EntityUrn,
    ) -> Result<EntityUsageEvent, String> {
        self.ensure_workspace_urn(&urn)?;
        let event = EntityUsageEvent::new(urn);
        append_ndjson(&self.usage_events_path(), &event)?;
        Ok(event)
    }

    pub fn record_rating(
        &self,
        urn: EntityUrn,
        input: EntityRatingInput,
    ) -> Result<EntityRatingEvent, String> {
        self.ensure_workspace_urn(&urn)?;
        let event = input.into_event(urn);
        append_ndjson(&self.rating_events_path(), &event)?;
        Ok(event)
    }

    /// Ingest a usage event attributed to a classified author. Author metadata
    /// is normalized by [`IngestAuthor`] before the event is persisted.
    pub fn ingest_usage(
        &self,
        author: &IngestAuthor,
        urn: EntityUrn,
    ) -> Result<EntityUsageEvent, String> {
        self.ensure_workspace_urn(&urn)?;
        let event = EntityUsageEvent::with_author(urn, author);
        append_ndjson(&self.usage_events_path(), &event)?;
        Ok(event)
    }

    /// Ingest a rating event attributed to a classified author. The author
    /// identity backfills a missing `agent_or_user_id` *before* the paired
    /// `session_id`/`agent_or_user_id` invariant is validated, so a caller may
    /// submit a `session_id` with an authenticated author and omit the actor id.
    pub fn ingest_rating(
        &self,
        author: &IngestAuthor,
        urn: EntityUrn,
        submission: EntityRatingSubmission,
    ) -> Result<EntityRatingEvent, String> {
        self.ensure_workspace_urn(&urn)?;

        let note_text = normalize_optional(submission.note_text);
        let session_id = normalize_optional(submission.session_id);
        // The author is always the attributable actor: backfill a missing
        // `agent_or_user_id` from the author identity before validation.
        let agent_or_user_id = normalize_optional(submission.agent_or_user_id)
            .or_else(|| author.id().map(str::to_string));
        let note_kind =
            resolve_note_kind(note_text.as_deref(), submission.note_kind)?;

        // A session reference still requires an attributable actor. Unlike the
        // direct (author-less) rating path, an actor without a session is
        // valid here because the author is the actor.
        if session_id.is_some() && agent_or_user_id.is_none() {
            return Err(
                "feedback session references require session_id and agent_or_user_id together"
                    .to_string(),
            );
        }

        let input = EntityRatingInput {
            rating: submission.rating,
            note_text,
            note_kind,
            session_id,
            agent_or_user_id,
        };
        let event = input.into_event_with_author(urn, author);
        append_ndjson(&self.rating_events_path(), &event)?;
        Ok(event)
    }

    /// Apply a retention policy to the persisted usage and rating logs using
    /// the current time as the age reference.
    pub fn apply_retention(
        &self,
        policy: &RetentionPolicy,
    ) -> Result<RetentionOutcome, String> {
        self.apply_retention_at(policy, Utc::now())
    }

    /// Apply a retention policy relative to an explicit reference time. The
    /// logs are rewritten in chronological order, keeping only events allowed
    /// by the policy. Applying the same policy twice removes nothing on the
    /// second pass.
    pub fn apply_retention_at(
        &self,
        policy: &RetentionPolicy,
        now: chrono::DateTime<Utc>,
    ) -> Result<RetentionOutcome, String> {
        let usage = prune_ndjson::<EntityUsageEvent>(
            &self.usage_events_path(),
            policy,
            now,
            |event| &event.timestamp,
        )?;
        let rating = prune_ndjson::<EntityRatingEvent>(
            &self.rating_events_path(),
            policy,
            now,
            |event| &event.timestamp,
        )?;

        Ok(RetentionOutcome { usage, rating })
    }

    pub fn summary_for(
        &self,
        urn: &EntityUrn,
    ) -> Result<EntityFeedbackSummary, String> {
        self.ensure_workspace_urn(urn)?;
        Ok(self.load_core()?.summary_for(urn))
    }

    pub fn entities_by_usage_frequency(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<EntityFeedbackSummary>, String> {
        Ok(self.load_core()?.entities_by_usage_frequency(limit))
    }

    pub fn low_rated_entities(
        &self,
    ) -> Result<Vec<EntityFeedbackSummary>, String> {
        Ok(self.load_core()?.low_rated_entities())
    }

    pub fn unresolved_note_entities(
        &self,
    ) -> Result<Vec<EntityFeedbackSummary>, String> {
        Ok(self.load_core()?.unresolved_note_entities())
    }

    fn load_core(&self) -> Result<EntityFeedbackCore, String> {
        let usage_events: Vec<EntityUsageEvent> =
            read_ndjson(&self.usage_events_path())?;
        let rating_events: Vec<EntityRatingEvent> =
            read_ndjson(&self.rating_events_path())?;

        // Validate rating invariants on read without discarding the stored
        // timestamp or author metadata.
        for event in &rating_events {
            EntityRatingInput::new(
                event.rating,
                event.note_text.clone(),
                event.note_kind,
                event.session_id.clone(),
                event.agent_or_user_id.clone(),
            )?;
        }

        Ok(EntityFeedbackCore {
            usage_events,
            rating_events,
        })
    }

    fn workspace_root(&self) -> PathBuf {
        self.root.join(&self.workspace_slug)
    }

    fn feedback_dir(&self) -> PathBuf {
        self.workspace_root().join(FEEDBACK_CORE_DIR)
    }

    fn usage_events_path(&self) -> PathBuf {
        self.feedback_dir().join(FEEDBACK_CORE_USAGE_FILE)
    }

    fn rating_events_path(&self) -> PathBuf {
        self.feedback_dir().join(FEEDBACK_CORE_RATING_FILE)
    }

    fn ensure_workspace_urn(
        &self,
        urn: &EntityUrn,
    ) -> Result<(), String> {
        if urn.workspace() == self.workspace_slug() {
            Ok(())
        } else {
            Err(format!(
                "entity urn workspace '{}' does not match store workspace '{}': {}",
                urn.workspace(),
                self.workspace_slug(),
                urn
            ))
        }
    }
}

impl EntityFeedbackCore {
    pub fn record_usage(
        &mut self,
        urn: EntityUrn,
    ) -> EntityUsageEvent {
        let event = EntityUsageEvent::new(urn);
        self.usage_events.push(event.clone());
        event
    }

    pub fn record_rating(
        &mut self,
        urn: EntityUrn,
        input: EntityRatingInput,
    ) -> EntityRatingEvent {
        let event = input.into_event(urn);
        self.rating_events.push(event.clone());
        event
    }

    pub fn summary_for(
        &self,
        urn: &EntityUrn,
    ) -> EntityFeedbackSummary {
        let mut summary = EntityFeedbackSummary::new(urn.clone());

        for event in self.usage_events.iter().filter(|e| &e.urn == urn) {
            summary.usage_count += 1;
            summary.last_used_at = Some(event.timestamp.clone());
        }
        for event in self.rating_events.iter().filter(|e| &e.urn == urn) {
            match event.rating {
                FeedbackRating::Helpful => summary.helpful_count += 1,
                FeedbackRating::Mixed => summary.mixed_count += 1,
                FeedbackRating::NotHelpful => summary.not_helpful_count += 1,
            }
            if event.has_note() {
                summary.note_count += 1;
                summary.unresolved_count += 1;
            }
            summary.last_rated_at = Some(event.timestamp.clone());
        }

        summary
    }

    pub fn entities_by_usage_frequency(
        &self,
        limit: Option<usize>,
    ) -> Vec<EntityFeedbackSummary> {
        let mut summaries = self.collect_all_summaries();
        summaries.sort_by(|left, right| {
            right
                .usage_count
                .cmp(&left.usage_count)
                .then_with(|| left.urn.as_str().cmp(&right.urn.as_str()))
        });
        if let Some(limit) = limit {
            summaries.truncate(limit);
        }
        summaries
    }

    pub fn low_rated_entities(&self) -> Vec<EntityFeedbackSummary> {
        self.collect_all_summaries()
            .into_iter()
            .filter(EntityFeedbackSummary::has_low_rating)
            .collect()
    }

    pub fn unresolved_note_entities(&self) -> Vec<EntityFeedbackSummary> {
        self.collect_all_summaries()
            .into_iter()
            .filter(EntityFeedbackSummary::has_unresolved_notes)
            .collect()
    }

    fn collect_all_summaries(&self) -> Vec<EntityFeedbackSummary> {
        let mut index: HashMap<EntityUrn, EntityFeedbackSummary> =
            HashMap::new();

        for event in &self.usage_events {
            let entry = index
                .entry(event.urn.clone())
                .or_insert_with(|| EntityFeedbackSummary::new(event.urn.clone()));
            entry.usage_count += 1;
            entry.last_used_at = Some(event.timestamp.clone());
        }

        for event in &self.rating_events {
            let entry = index
                .entry(event.urn.clone())
                .or_insert_with(|| EntityFeedbackSummary::new(event.urn.clone()));
            match event.rating {
                FeedbackRating::Helpful => entry.helpful_count += 1,
                FeedbackRating::Mixed => entry.mixed_count += 1,
                FeedbackRating::NotHelpful => entry.not_helpful_count += 1,
            }
            if event.has_note() {
                entry.note_count += 1;
                entry.unresolved_count += 1;
            }
            entry.last_rated_at = Some(event.timestamp.clone());
        }

        let mut summaries: Vec<_> = index.into_values().collect();
        summaries.sort_by(|left, right| left.urn.as_str().cmp(&right.urn.as_str()));
        summaries
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

/// Resolve the effective note kind for a feedback event: a note kind requires
/// note text, and note text without an explicit kind defaults to `Note`.
fn resolve_note_kind(
    note_text: Option<&str>,
    note_kind: Option<FeedbackNoteKind>,
) -> Result<Option<FeedbackNoteKind>, String> {
    match (note_text, note_kind) {
        (Some(_), Some(kind)) => Ok(Some(kind)),
        (Some(_), None) => Ok(Some(FeedbackNoteKind::Note)),
        (None, None) => Ok(None),
        (None, Some(_)) => {
            Err("feedback note kind requires feedback note text".to_string())
        },
    }
}

fn append_ndjson<T: Serialize>(
    path: &Path,
    item: &T,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create feedback core directory {}: {err}",
                parent.display()
            )
        })?;
    }

    let line = serde_json::to_string(item)
        .map_err(|err| format!("failed to serialize ndjson item: {err}"))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| {
            format!(
                "failed to open feedback core log {}: {err}",
                path.display()
            )
        })?;
    writeln!(file, "{line}").map_err(|err| {
        format!(
            "failed to append feedback core log {}: {err}",
            path.display()
        )
    })
}

fn read_ndjson<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path).map_err(|err| {
        format!(
            "failed to open feedback core log {}: {err}",
            path.display()
        )
    })?;
    let reader = BufReader::new(file);
    let mut items = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(|err| {
            format!(
                "failed reading feedback core log {} line {}: {err}",
                path.display(),
                index + 1
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let item = serde_json::from_str::<T>(&line).map_err(|err| {
            format!(
                "invalid feedback core event {} line {}: {err}",
                path.display(),
                index + 1
            )
        })?;
        items.push(item);
    }

    Ok(items)
}

/// Rewrite an NDJSON event log in place, keeping only the events allowed by a
/// retention policy. Events are assumed to be appended in chronological order;
/// the age filter drops events older than `now - max_age`, and `max_events`
/// then keeps the most recent surviving events. Returns retained/removed
/// counts. Applying the same policy again removes nothing.
fn prune_ndjson<T>(
    path: &Path,
    policy: &RetentionPolicy,
    now: chrono::DateTime<Utc>,
    timestamp_of: impl Fn(&T) -> &str,
) -> Result<RetentionKindOutcome, String>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    let events: Vec<T> = read_ndjson(path)?;
    let original = events.len();
    if original == 0 {
        return Ok(RetentionKindOutcome::default());
    }

    let mut kept: Vec<T> = Vec::with_capacity(original);
    for event in events {
        if let Some(max_age) = policy.max_age {
            let raw = timestamp_of(&event);
            let parsed =
                chrono::DateTime::parse_from_rfc3339(raw).map_err(|err| {
                    format!(
                        "invalid feedback core timestamp '{raw}' in {}: {err}",
                        path.display()
                    )
                })?;
            if now.signed_duration_since(parsed.with_timezone(&Utc)) > max_age {
                continue;
            }
        }
        kept.push(event);
    }

    if let Some(max_events) = policy.max_events
        && kept.len() > max_events
    {
        let overflow = kept.len() - max_events;
        kept.drain(0..overflow);
    }

    let retained = kept.len();
    let removed = original - retained;

    if removed > 0 {
        rewrite_ndjson(path, &kept)?;
    }

    Ok(RetentionKindOutcome { retained, removed })
}

/// Atomically replace an NDJSON log with the provided items.
fn rewrite_ndjson<T: Serialize>(
    path: &Path,
    items: &[T],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create feedback core directory {}: {err}",
                parent.display()
            )
        })?;
    }

    let mut buffer = String::new();
    for item in items {
        let line = serde_json::to_string(item)
            .map_err(|err| format!("failed to serialize ndjson item: {err}"))?;
        buffer.push_str(&line);
        buffer.push('\n');
    }

    let tmp_path = path.with_extension("ndjson.tmp");
    fs::write(&tmp_path, &buffer).map_err(|err| {
        format!(
            "failed to write feedback core log {}: {err}",
            tmp_path.display()
        )
    })?;
    fs::rename(&tmp_path, path).map_err(|err| {
        format!(
            "failed to replace feedback core log {}: {err}",
            path.display()
        )
    })
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

    #[test]
    fn parses_ce_urn_and_keeps_supported_store_segments() {
        let rule_urn: EntityUrn = "ce://memory-api/rule/rule-123".parse().unwrap();
        let spec_urn: EntityUrn = "ce://memory-api/spec/spec-123".parse().unwrap();
        let ticket_urn: EntityUrn = "ce://memory-api/ticket/ticket-123".parse().unwrap();

        assert_eq!(rule_urn.store(), "rule");
        assert_eq!(spec_urn.store(), "spec");
        assert_eq!(ticket_urn.store(), "ticket");
    }

    #[test]
    fn entity_feedback_core_ranks_usage_and_finds_low_rated_and_unresolved() {
        let mut core = EntityFeedbackCore::default();
        let rule_urn: EntityUrn = "ce://memory-api/rule/rule-123".parse().unwrap();
        let spec_urn: EntityUrn = "ce://memory-api/spec/spec-123".parse().unwrap();

        core.record_usage(rule_urn.clone());
        core.record_usage(rule_urn.clone());
        core.record_usage(spec_urn.clone());

        core.record_rating(
            rule_urn.clone(),
            EntityRatingInput::new(
                FeedbackRating::NotHelpful,
                Some("missing example".to_string()),
                None,
                None,
                None,
            )
            .unwrap(),
        );
        core.record_rating(
            spec_urn.clone(),
            EntityRatingInput::new(
                FeedbackRating::Helpful,
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        );

        let by_usage = core.entities_by_usage_frequency(None);
        assert_eq!(by_usage[0].urn, rule_urn);
        assert_eq!(by_usage[0].usage_count, 2);

        let low_rated = core.low_rated_entities();
        assert_eq!(low_rated.len(), 1);
        assert_eq!(low_rated[0].urn, rule_urn);

        let unresolved = core.unresolved_note_entities();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].urn, rule_urn);
    }

    #[test]
    fn feedback_store_persists_usage_and_rating_for_rule_and_spec_urns() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            EntityFeedbackStore::new(dir.path(), "memory-api").unwrap();

        let rule_urn = EntityUrn::rule("memory-api", "rule-123").unwrap();
        let spec_urn = EntityUrn::spec("memory-api", "spec-123").unwrap();

        store.record_usage(rule_urn.clone()).unwrap();
        store.record_usage(rule_urn.clone()).unwrap();
        store.record_usage(spec_urn.clone()).unwrap();

        store
            .record_rating(
                rule_urn.clone(),
                EntityRatingInput::new(
                    FeedbackRating::NotHelpful,
                    Some("too vague".to_string()),
                    Some(FeedbackNoteKind::Suggestion),
                    None,
                    None,
                )
                .unwrap(),
            )
            .unwrap();
        store
            .record_rating(
                spec_urn.clone(),
                EntityRatingInput::new(
                    FeedbackRating::Helpful,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap(),
            )
            .unwrap();

        let by_usage = store.entities_by_usage_frequency(None).unwrap();
        assert_eq!(by_usage[0].urn, rule_urn);
        assert_eq!(by_usage[0].usage_count, 2);

        let low_rated = store.low_rated_entities().unwrap();
        assert_eq!(low_rated.len(), 1);
        assert_eq!(low_rated[0].urn, rule_urn);

        let unresolved = store.unresolved_note_entities().unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].urn, rule_urn);

        let spec_summary = store.summary_for(&spec_urn).unwrap();
        assert_eq!(spec_summary.usage_count, 1);
        assert_eq!(spec_summary.helpful_count, 1);
    }

    #[test]
    fn feedback_store_accepts_ticket_urn_extension_point() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            EntityFeedbackStore::new(dir.path(), "memory-api").unwrap();
        let ticket_urn =
            EntityUrn::ticket("memory-api", "ticket-123").unwrap();

        store.record_usage(ticket_urn.clone()).unwrap();
        let summary = store.summary_for(&ticket_urn).unwrap();

        assert_eq!(summary.usage_count, 1);
    }

    #[test]
    fn ingest_author_normalizes_id_and_requires_privileged_identity() {
        let human = IngestAuthor::human(Some("  ada  ".to_string())).unwrap();
        assert_eq!(human.kind(), FeedbackAuthorKind::Human);
        assert_eq!(human.id(), Some("ada"));

        // Human may omit identity; empty-after-trim is treated as absent.
        let anon = IngestAuthor::human(Some("   ".to_string())).unwrap();
        assert_eq!(anon.id(), None);

        // Privileged agents must supply a non-empty identity.
        let err = IngestAuthor::new(
            FeedbackAuthorKind::PrivilegedAgent,
            Some("  ".to_string()),
        )
        .unwrap_err();
        assert!(err.contains("privileged-agent ingestion requires"));

        let agent = IngestAuthor::privileged_agent("copilot").unwrap();
        assert_eq!(agent.kind(), FeedbackAuthorKind::PrivilegedAgent);
        assert_eq!(agent.id(), Some("copilot"));
    }

    #[test]
    fn author_kind_round_trips_through_persisted_events() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            EntityFeedbackStore::new(dir.path(), "memory-api").unwrap();
        let rule_urn = EntityUrn::rule("memory-api", "rule-a").unwrap();
        let agent = IngestAuthor::privileged_agent("copilot").unwrap();

        let usage = store.ingest_usage(&agent, rule_urn.clone()).unwrap();
        assert_eq!(usage.author_kind, Some(FeedbackAuthorKind::PrivilegedAgent));
        assert_eq!(usage.author_id.as_deref(), Some("copilot"));

        // Rating input without an explicit actor backfills from the author.
        let rating = store
            .ingest_rating(
                &agent,
                rule_urn.clone(),
                EntityRatingSubmission::new(FeedbackRating::Helpful),
            )
            .unwrap();
        assert_eq!(rating.author_kind, Some(FeedbackAuthorKind::PrivilegedAgent));
        assert_eq!(rating.agent_or_user_id.as_deref(), Some("copilot"));

        // Reload from disk to confirm the author metadata persisted.
        let reloaded: Vec<EntityUsageEvent> =
            read_ndjson(&store.usage_events_path()).unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(
            reloaded[0].author_kind,
            Some(FeedbackAuthorKind::PrivilegedAgent)
        );
    }

    #[test]
    fn ingest_rating_backfills_author_for_session_scoped_rating() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            EntityFeedbackStore::new(dir.path(), "memory-api").unwrap();
        let rule_urn = EntityUrn::rule("memory-api", "rule-session").unwrap();
        let agent = IngestAuthor::privileged_agent("copilot").unwrap();

        // Session-scoped rating with the actor id omitted: the author must
        // backfill `agent_or_user_id` before the paired-session invariant is
        // validated, so this call succeeds without repeating the author id.
        let mut submission = EntityRatingSubmission::new(FeedbackRating::Mixed);
        submission.session_id = Some("session-42".to_string());

        let rating = store
            .ingest_rating(&agent, rule_urn.clone(), submission)
            .unwrap();

        assert_eq!(rating.session_id.as_deref(), Some("session-42"));
        assert_eq!(rating.agent_or_user_id.as_deref(), Some("copilot"));
        assert_eq!(rating.author_kind, Some(FeedbackAuthorKind::PrivilegedAgent));

        // The persisted event carries the backfilled actor and session.
        let reloaded: Vec<EntityRatingEvent> =
            read_ndjson(&store.rating_events_path()).unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].session_id.as_deref(), Some("session-42"));
        assert_eq!(reloaded[0].agent_or_user_id.as_deref(), Some("copilot"));

        // A human author with no identity still cannot attribute a session.
        let anon = IngestAuthor::human(None).unwrap();
        let mut orphan = EntityRatingSubmission::new(FeedbackRating::Mixed);
        orphan.session_id = Some("session-99".to_string());
        let err = store.ingest_rating(&anon, rule_urn, orphan).unwrap_err();
        assert!(err.contains("session_id and agent_or_user_id together"));
    }

    #[test]
    fn retention_prunes_by_age_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            EntityFeedbackStore::new(dir.path(), "memory-api").unwrap();
        let urn = EntityUrn::rule("memory-api", "rule-age").unwrap();
        let path = store.usage_events_path();

        // Write events by hand so timestamps are deterministic.
        let old = EntityUsageEvent {
            timestamp: "2000-01-01T00:00:00+00:00".to_string(),
            urn: urn.clone(),
            author_kind: None,
            author_id: None,
        };
        let recent = EntityUsageEvent {
            timestamp: "2025-01-01T00:00:00+00:00".to_string(),
            urn: urn.clone(),
            author_kind: None,
            author_id: None,
        };
        append_ndjson(&path, &old).unwrap();
        append_ndjson(&path, &recent).unwrap();

        let now = chrono::DateTime::parse_from_rfc3339("2025-01-10T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let policy = RetentionPolicy::max_age_days(30);

        let outcome = store.apply_retention_at(&policy, now).unwrap();
        assert_eq!(outcome.usage.retained, 1);
        assert_eq!(outcome.usage.removed, 1);

        let kept: Vec<EntityUsageEvent> = read_ndjson(&path).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].timestamp, recent.timestamp);

        // Second pass removes nothing.
        let repeat = store.apply_retention_at(&policy, now).unwrap();
        assert_eq!(repeat.total_removed(), 0);
    }

    #[test]
    fn retention_keeps_most_recent_events_by_count() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            EntityFeedbackStore::new(dir.path(), "memory-api").unwrap();
        let urn = EntityUrn::rule("memory-api", "rule-count").unwrap();
        let path = store.usage_events_path();

        for day in 1..=5 {
            let event = EntityUsageEvent {
                timestamp: format!("2025-01-0{day}T00:00:00+00:00"),
                urn: urn.clone(),
                author_kind: None,
                author_id: None,
            };
            append_ndjson(&path, &event).unwrap();
        }

        let outcome = store
            .apply_retention_at(
                &RetentionPolicy::max_events(2),
                Utc::now(),
            )
            .unwrap();
        assert_eq!(outcome.usage.retained, 2);
        assert_eq!(outcome.usage.removed, 3);

        let kept: Vec<EntityUsageEvent> = read_ndjson(&path).unwrap();
        assert_eq!(kept.len(), 2);
        // The two most recent (chronologically last) events survive.
        assert_eq!(kept[0].timestamp, "2025-01-04T00:00:00+00:00");
        assert_eq!(kept[1].timestamp, "2025-01-05T00:00:00+00:00");
    }
}

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
