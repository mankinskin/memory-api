//! Structured feedback signal extraction from captured sessions.
//!
//! Signals are derived **only** from structured metadata — never from
//! free-text heuristics over message content. Content-aware analysis (for
//! example embedding-based reasoning about message context) is intentionally
//! deferred. The previous implementation tokenized message text and matched
//! "confusion markers" plus a two-token keyword overlap against rule bodies,
//! which produced large volumes of false positives when run over captured
//! transcripts. That heuristic has been removed entirely.
//!
//! Two distinct structured sources are mined:
//!
//! - [`SessionTurn`] metadata ([`crate::SessionTurnEventMeta`]), for
//!   [`FeedbackSignalKind::FailedToolCall`].
//! - Captured tool-execution [`crate::CopilotHookEvent`]s, for
//!   [`FeedbackSignalKind::ExplicitIngestion`].
//!
//! These are separate sources, not an oversight: grounding against real
//! captured `.session/sessions/*` transcripts shows that tool call/result
//! pairs are recorded as session **events** (`tool.execution_start` /
//! `tool.execution_complete`; legacy captures may also include
//! `tool.execution_result`), not as `SessionTurn`s — every
//! committed session transcript has zero turns with `role: tool`. A detector
//! that only inspected `SessionTurn`s would therefore never fire on real
//! data for tool-call signals; `ExplicitIngestion` mining reads the events
//! list directly instead of guessing that turn-based metadata is populated.

use std::str::FromStr;

use feedback_api::{
    EntityUrn,
    FeedbackEntry,
    FeedbackNoteKind,
    FeedbackProvenance,
    FeedbackRating,
    FeedbackSource,
};
use serde_json::Value;

use crate::{
    CopilotHookEvent,
    SessionRole,
    SessionTurn,
};

/// Tool name suffix identifying the `feedback-mcp` `feedback_ingest` tool.
/// Captured `tool_name` values are session-scoped and prefixed by the
/// client (observed as `mcp_<server-slug>_feedback_ingest` in committed
/// transcripts), so matching is done on the suffix rather than an exact
/// string. `feedback_mine` is intentionally excluded: it persists a fixed
/// placeholder entry (a leftover from the removed transcript miner) rather
/// than an agent/user-supplied target + rating + note, so it carries no
/// mineable ingestion payload.
const FEEDBACK_INGEST_TOOL_SUFFIX: &str = "feedback_ingest";

/// Classification of a structured feedback signal detected in a captured
/// session.
///
/// Only signals that can be derived unambiguously from captured structured
/// metadata are represented here.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum FeedbackSignalKind {
    /// A tool invocation whose captured `tool_success` flag was `false`.
    FailedToolCall,
    /// A captured `feedback_ingest` tool call, carrying its structured
    /// arguments in [`StructuredFeedbackSignal::ingestion`].
    ExplicitIngestion,
}

/// The structured arguments captured for an `ExplicitIngestion` signal,
/// copied verbatim from `tool_arguments_json` (the flat parameter object the
/// tool was invoked with). Fields are `Option` because a captured call may
/// be missing an optional argument, or the argument may not have serialized
/// as a plain string; no value here is inferred or guessed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExplicitIngestionArgs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// A backtraceable feedback signal extracted from structured session
/// metadata.
///
/// Every field is sourced from captured metadata so a downstream consumer can
/// trace the signal back to the exact turn and/or tool call that produced
/// it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StructuredFeedbackSignal {
    /// What kind of structured signal was detected.
    pub kind: FeedbackSignalKind,
    /// Turn sequence within the captured session, when the signal was
    /// derived from a `SessionTurn`. `None` for event-derived signals
    /// (`ExplicitIngestion`), which have no numbered turn to reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<usize>,
    /// Name of the tool associated with the signal, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Captured tool-call id, enabling backtracing to the originating call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Captured event id, enabling backtracing to the originating event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    /// The captured `tool_success` flag for the originating call, when
    /// known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_success: Option<bool>,
    /// Populated only for `ExplicitIngestion` signals: the structured
    /// arguments the `feedback_ingest` tool was invoked with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingestion: Option<ExplicitIngestionArgs>,
    /// Populated only for `FailedToolCall` signals produced by
    /// [`mine_failed_tool_call_signals`]: the outcome of mapping the failing
    /// call to a feedback target entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mapping: Option<FailedToolCallMapping>,
}

/// Extract structured feedback signals from a session's turns.
///
/// This is a pure, side-effect-free classification over structured metadata.
/// It performs no store writes and creates no tickets; callers decide how to
/// act on the returned signals.
pub fn mine_structured_feedback_signals(
    turns: &[SessionTurn]
) -> Vec<StructuredFeedbackSignal> {
    turns.iter().filter_map(detect_signal).collect()
}

fn detect_signal(turn: &SessionTurn) -> Option<StructuredFeedbackSignal> {
    let meta = turn.event_meta.as_ref()?;

    // Only structured tool outcomes are trusted. A `tool_success` of `false`
    // is an explicit failure flag recorded at capture time; no natural-language
    // interpretation is involved.
    if turn.role != SessionRole::Tool || meta.tool_success != Some(false) {
        return None;
    }

    Some(StructuredFeedbackSignal {
        kind: FeedbackSignalKind::FailedToolCall,
        sequence: Some(turn.sequence),
        tool_name: turn.tool_name.clone(),
        tool_call_id: meta.tool_call_id.clone(),
        event_id: meta.event_id.clone(),
        tool_success: meta.tool_success,
        ingestion: None,
        mapping: None,
    })
}

/// Extract explicit feedback-ingestion signals from a session's captured
/// tool-execution events.
///
/// This reads canonical `tool.execution_complete` events (plus legacy
/// `tool.execution_result` events; see the module docs), matches on the
/// captured `feedback_ingest` tool name, and copies the captured arguments
/// verbatim. This is a pure, side-effect-free classification; it performs no
/// store writes.
pub fn mine_explicit_ingestion_signals(
    events: &[CopilotHookEvent]
) -> Vec<StructuredFeedbackSignal> {
    canonicalize_outcome_events(events)
        .iter()
        .filter_map(detect_explicit_ingestion)
        .collect()
}

fn detect_explicit_ingestion(
    event: &CopilotHookEvent
) -> Option<StructuredFeedbackSignal> {
    if !is_tool_execution_outcome(event.event_type.as_deref()) {
        return None;
    }
    let tool_name = event.tool_name.as_deref()?;
    if !tool_name.ends_with(FEEDBACK_INGEST_TOOL_SUFFIX) {
        return None;
    }

    let arguments = event.tool_arguments_json.as_ref();
    let ingestion = ExplicitIngestionArgs {
        target: json_str(arguments, "target"),
        source: json_str(arguments, "source"),
        rating: json_str(arguments, "rating"),
        note: json_str(arguments, "note"),
        note_kind: json_str(arguments, "note_kind"),
        session_id: json_str(arguments, "session_id"),
        author: json_str(arguments, "author"),
    };

    Some(StructuredFeedbackSignal {
        kind: FeedbackSignalKind::ExplicitIngestion,
        sequence: None,
        tool_name: Some(tool_name.to_string()),
        tool_call_id: event.tool_call_id.clone(),
        event_id: event.event_id.clone(),
        tool_success: event.tool_success,
        ingestion: Some(ingestion),
        mapping: None,
    })
}

fn json_str(
    value: Option<&Value>,
    key: &str,
) -> Option<String> {
    value?.get(key)?.as_str().map(str::to_string)
}

/// Outcome of mapping a failed tool call to a feedback target entity.
///
/// # Policy (grounded against real `.session/sessions/*` transcripts)
///
/// Investigating every captured `tool.execution_result` event with
/// `tool_success == false` across the committed session store (115 failed
/// calls out of 5,031 captured tool results, spanning 61 sessions) found:
///
/// - The largest failure category (~46%) is generic file/dev tools
///   (`read_file`, `apply_patch`, `create_file`, `grep_search`, `list_dir`,
///   `run_in_terminal`) whose arguments reference a filesystem path, not an
///   entity in any of `feedback-api`'s supported `EntityUrn` stores (only
///   `rule`, `spec`, `ticket` exist — grounded by grepping every
///   `EntityUrn::new`/`::rule`/`::spec`/`::ticket` call site in this repo).
/// - `test-mcp`'s `test_record_execution` fails a further 8 times; its
///   `validation_spec_id` likewise has no corresponding `EntityUrn` store.
/// - `ticket-mcp` methods that take an existing ticket's id fail 42 times
///   combined (`board_check_out` 23, `get_ticket` 6, `update_ticket` 4,
///   `get_ticket_description` 3, `board_check_in` 2, plus board file/rename
///   ops) — these unambiguously reference one ticket.
/// - `create_ticket` fails 7 times but creates a *new* entity, so it has no
///   existing-entity id to map to.
/// - `add_edge`/`remove_edge` reference two candidate tickets (`from`/`to`)
///   with no principled way to prefer one without guessing.
///
/// Decision: map to an entity only when the tool is a known
/// entity-domain method (today: `ticket-mcp`, keyed on its documented
/// single-ticket id argument) **and** that argument is present. Every other
/// case is an explicit, typed [`UnmappedReason`] rather than a silent
/// fallback or a guess.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum FailedToolCallMapping {
    /// The failing call's arguments unambiguously reference a single known
    /// entity.
    Entity { urn: EntityUrn },
    /// No single entity could be confidently attributed; see
    /// [`UnmappedReason`] for why.
    Unmapped { reason: UnmappedReason },
}

/// Why a failed tool call could not be mapped to a single entity. Kept as
/// distinct variants (rather than one opaque "unmapped" marker) so a caller
/// or audit can distinguish "we don't recognize this tool" from "we
/// recognize it but this call has no entity-identifying argument" from
/// "there were multiple candidate entities and picking one would be a
/// guess" from "this tool's domain has no corresponding entity store".
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum UnmappedReason {
    /// The captured `tool_name` (after allowing for a session-scoped client
    /// alias prefix) is not in the known entity-domain method table.
    UnknownTool,
    /// The tool is a known entity-domain method, but this call's captured
    /// arguments carry no entity-identifying field (for example
    /// `create_ticket`, which creates a new entity rather than referencing
    /// an existing one).
    NoEntityIdArgument,
    /// The call's arguments reference more than one plausible entity (for
    /// example an edge `from`/`to` pair) and picking one would be a guess.
    AmbiguousMultipleCandidates,
    /// The tool's domain has no corresponding `EntityUrn` store today (for
    /// example file-path-based dev tools, or `test-mcp` validation specs).
    NoSupportedEntityStore,
}

/// `ticket-mcp` methods whose single-ticket identifier argument is named
/// `ticket_id`, grounded against captured argument keys on real failed
/// calls.
const TICKET_ID_KEYED_METHODS: &[&str] = &[
    "board_check_in",
    "board_check_out",
    "board_update_files",
    "board_rename_file",
];

/// `ticket-mcp` methods whose single-ticket identifier argument is named
/// `id`, grounded against captured argument keys on real failed calls.
const TICKET_ID_ALIAS_KEYED_METHODS: &[&str] = &[
    "get_ticket",
    "get_ticket_description",
    "update_ticket",
    "close_ticket",
    "cancel_ticket",
    "delete_ticket",
];

/// Known entity-domain methods observed to fail without a usable
/// single-entity id argument (creates a new entity, or references more than
/// one candidate).
const NO_ENTITY_ID_METHODS: &[&str] = &["create_ticket"];
const AMBIGUOUS_METHODS: &[&str] = &["add_edge", "remove_edge"];

/// Known non-entity-domain tools observed among real failed calls: generic
/// file/dev tools and `test-mcp`'s validation-execution recorder, none of
/// which target a `rule`/`spec`/`ticket` `EntityUrn`.
const NO_SUPPORTED_STORE_METHODS: &[&str] = &[
    "read_file",
    "apply_patch",
    "create_file",
    "grep_search",
    "list_dir",
    "run_in_terminal",
    "test_record_execution",
];

/// Map a failed tool call to a feedback target entity per the policy
/// documented on [`FailedToolCallMapping`].
///
/// `tool_name` is matched by suffix because captured names are prefixed by a
/// session-scoped client alias (observed as `mcp_<alias>_<method>` in
/// committed transcripts) rather than a stable server identifier.
/// `workspace_slug` must be supplied by the caller from session metadata
/// (`SessionMetadata::workspace_slug`); the `workspace` argument captured on
/// the call itself is a filesystem path, not the `EntityUrn` workspace slug.
pub fn map_failed_tool_call_to_entity(
    tool_name: Option<&str>,
    tool_arguments_json: Option<&Value>,
    workspace_slug: &str,
) -> FailedToolCallMapping {
    let Some(tool_name) = tool_name else {
        return FailedToolCallMapping::Unmapped {
            reason: UnmappedReason::UnknownTool,
        };
    };

    if AMBIGUOUS_METHODS.iter().any(|m| tool_name.ends_with(m)) {
        return FailedToolCallMapping::Unmapped {
            reason: UnmappedReason::AmbiguousMultipleCandidates,
        };
    }
    if NO_SUPPORTED_STORE_METHODS
        .iter()
        .any(|m| tool_name.ends_with(m))
    {
        return FailedToolCallMapping::Unmapped {
            reason: UnmappedReason::NoSupportedEntityStore,
        };
    }
    if NO_ENTITY_ID_METHODS.iter().any(|m| tool_name.ends_with(m)) {
        return FailedToolCallMapping::Unmapped {
            reason: UnmappedReason::NoEntityIdArgument,
        };
    }

    let id_key = if TICKET_ID_KEYED_METHODS
        .iter()
        .any(|m| tool_name.ends_with(m))
    {
        Some("ticket_id")
    } else if TICKET_ID_ALIAS_KEYED_METHODS
        .iter()
        .any(|m| tool_name.ends_with(m))
    {
        Some("id")
    } else {
        None
    };

    let Some(id_key) = id_key else {
        return FailedToolCallMapping::Unmapped {
            reason: UnmappedReason::UnknownTool,
        };
    };

    match json_str(tool_arguments_json, id_key)
        .and_then(|id| EntityUrn::ticket(workspace_slug, id).ok())
    {
        Some(urn) => FailedToolCallMapping::Entity { urn },
        None => FailedToolCallMapping::Unmapped {
            reason: UnmappedReason::NoEntityIdArgument,
        },
    }
}

/// Extract failed-tool-call signals from a session's captured tool-execution
/// events, with each signal's [`FailedToolCallMapping`] resolved.
///
/// This reads canonical `tool.execution_complete` events (plus legacy
/// `tool.execution_result`) — see the module docs for why events, not turns,
/// carry this data on real captured sessions. This is a pure,
/// side-effect-free classification; it performs no store writes and creates no
/// tickets.
pub fn mine_failed_tool_call_signals(
    events: &[CopilotHookEvent],
    workspace_slug: &str,
) -> Vec<StructuredFeedbackSignal> {
    canonicalize_outcome_events(events)
        .iter()
        .filter_map(|event| detect_failed_tool_call(event, workspace_slug))
        .collect()
}

fn detect_failed_tool_call(
    event: &CopilotHookEvent,
    workspace_slug: &str,
) -> Option<StructuredFeedbackSignal> {
    if !is_tool_execution_outcome(event.event_type.as_deref()) {
        return None;
    }
    if event.tool_success != Some(false) {
        return None;
    }

    let mapping = map_failed_tool_call_to_entity(
        event.tool_name.as_deref(),
        event.tool_arguments_json.as_ref(),
        workspace_slug,
    );

    Some(StructuredFeedbackSignal {
        kind: FeedbackSignalKind::FailedToolCall,
        sequence: None,
        tool_name: event.tool_name.clone(),
        tool_call_id: event.tool_call_id.clone(),
        event_id: event.event_id.clone(),
        tool_success: event.tool_success,
        ingestion: None,
        mapping: Some(mapping),
    })
}

fn is_tool_execution_outcome(event_type: Option<&str>) -> bool {
    matches!(
        event_type,
        Some("tool.execution_result")
            | Some("tool_execution_result")
            | Some("tool.execution_complete")
            | Some("tool_execution_complete")
    )
}

fn canonicalize_outcome_events(
    events: &[CopilotHookEvent]
) -> Vec<CopilotHookEvent> {
    let mut complete_tool_calls = std::collections::BTreeSet::<String>::new();
    for event in events {
        if is_tool_execution_complete(event.event_type.as_deref()) {
            if let Some(tool_call_id) = event.tool_call_id.as_ref() {
                complete_tool_calls.insert(tool_call_id.clone());
            }
        }
    }

    let mut normalized = Vec::with_capacity(events.len());
    for event in events {
        if is_tool_execution_result(event.event_type.as_deref())
            && event
                .tool_call_id
                .as_ref()
                .is_some_and(|id| complete_tool_calls.contains(id))
        {
            continue;
        }
        normalized.push(event.clone());
    }

    normalized
}

fn is_tool_execution_complete(event_type: Option<&str>) -> bool {
    matches!(
        event_type,
        Some("tool.execution_complete") | Some("tool_execution_complete")
    )
}

fn is_tool_execution_result(event_type: Option<&str>) -> bool {
    matches!(
        event_type,
        Some("tool.execution_result") | Some("tool_execution_result")
    )
}

/// Build a backtraceable [`FeedbackEntry`] recovering an `ExplicitIngestion`
/// signal whose live `feedback_ingest` call did **not** successfully
/// persist.
///
/// A signal whose captured `tool_success` is `Some(true)` is skipped
/// (returns `Ok(None)`) rather than recorded: the live tool call already
/// persisted its own entry via `EntityFeedbackStore` at call time, so
/// recording it again here would create a duplicate — the exact failure
/// mode (false-positive/duplicate auto-created entries) this feedback-ring
/// hardening effort exists to eliminate. Signals with missing required
/// arguments (`target`, `source`) are also skipped rather than guessed at.
pub fn recover_feedback_entry_from_signal(
    signal: &StructuredFeedbackSignal,
    fallback_session_id: Option<String>,
) -> Result<Option<FeedbackEntry>, String> {
    if signal.kind != FeedbackSignalKind::ExplicitIngestion {
        return Ok(None);
    }
    if signal.tool_success == Some(true) {
        return Ok(None);
    }
    let Some(ingestion) = signal.ingestion.as_ref() else {
        return Ok(None);
    };
    let (Some(target_raw), Some(source_raw)) =
        (ingestion.target.as_deref(), ingestion.source.as_deref())
    else {
        return Ok(None);
    };

    let source = FeedbackSource::from_str(source_raw)?;
    let target = EntityUrn::from_str(target_raw)?;
    let rating = ingestion
        .rating
        .as_deref()
        .map(FeedbackRating::from_str)
        .transpose()?;
    let note_kind = ingestion
        .note_kind
        .as_deref()
        .map(FeedbackNoteKind::from_str)
        .transpose()?;
    let session_id = ingestion.session_id.clone().or(fallback_session_id);

    let provenance = FeedbackProvenance::from_session_turn(
        session_id,
        ingestion.author.clone(),
        None,
        signal.sequence,
        signal.tool_call_id.clone(),
    )?;

    let entry = FeedbackEntry::new(
        source,
        target,
        rating,
        ingestion.note.clone(),
        note_kind,
        provenance,
    )?;
    Ok(Some(entry))
}

/// Deterministic, order-preserving, deduplicated discovery queue of feedback
/// target entities.
///
/// Entities are enqueued in first-discovery order; a given [`EntityUrn`] is
/// enqueued at most once (first discovery wins — later re-references of the
/// same entity are deduped rather than reprocessed). This is the
/// deterministic breadth-first iteration the structured feedback ring
/// requires: process signals in a fixed order, and append newly-discovered
/// entities to the queue as they are found, never revisiting one already
/// seen.
///
/// Today's signal kinds (`ExplicitIngestion`'s `target`, `FailedToolCall`'s
/// resolved [`FailedToolCallMapping::Entity`]) each reference at most one
/// entity directly, with no further related entities to expand into — so
/// this is currently the documented acceptable alternative of "mine only
/// the entities detected at the beginning" rather than a multi-level BFS
/// traversal. The queue abstraction is kept independent of any particular
/// signal kind so a future signal that discovers *related* entities (for
/// example a ticket's `depends_on` links) can enqueue onto it without
/// changing the ordering/dedup contract relied on here.
#[derive(Debug, Default)]
pub struct EntityDiscoveryQueue {
    seen: std::collections::HashSet<EntityUrn>,
    order: Vec<EntityUrn>,
}

impl EntityDiscoveryQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue an entity if it has not been seen before. Returns `true` if
    /// this was a new entity (now queued), `false` if it was already
    /// discovered and is therefore skipped.
    pub fn enqueue(
        &mut self,
        urn: EntityUrn,
    ) -> bool {
        if self.seen.insert(urn.clone()) {
            self.order.push(urn);
            true
        } else {
            false
        }
    }

    /// Consume the queue, returning discovered entities in first-discovery
    /// order.
    pub fn into_ordered(self) -> Vec<EntityUrn> {
        self.order
    }
}

/// Discover the distinct feedback target entities referenced by a session's
/// structured feedback signals, in deterministic first-discovery order.
///
/// This is a pure function over already-mined signals (see
/// [`mine_failed_tool_call_signals`] and [`mine_explicit_ingestion_signals`])
/// — it performs no store writes and creates no tickets.
pub fn discover_entities_from_signals(
    signals: &[StructuredFeedbackSignal]
) -> Vec<EntityUrn> {
    let mut queue = EntityDiscoveryQueue::new();
    for signal in signals {
        for urn in entity_refs(signal) {
            queue.enqueue(urn);
        }
    }
    queue.into_ordered()
}

fn entity_refs(signal: &StructuredFeedbackSignal) -> Vec<EntityUrn> {
    let mut refs = Vec::new();
    if let Some(FailedToolCallMapping::Entity { urn }) = &signal.mapping {
        refs.push(urn.clone());
    }
    if let Some(target_urn) = signal
        .ingestion
        .as_ref()
        .and_then(|args| args.target.as_deref())
        .and_then(|raw| EntityUrn::from_str(raw).ok())
    {
        refs.push(target_urn);
    }
    refs
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::SessionTurnEventMeta;

    fn tool_turn(
        sequence: usize,
        tool_success: Option<bool>,
    ) -> SessionTurn {
        SessionTurn {
            sequence,
            role: SessionRole::Tool,
            content: String::new(),
            captured_at: Utc::now(),
            tool_name: Some("get_ticket".to_string()),
            model: None,
            event_meta: Some(SessionTurnEventMeta {
                event_id: Some(format!("evt-{sequence}")),
                parent_event_id: None,
                event_type: Some("tool.result".to_string()),
                turn_id: None,
                message_id: None,
                tool_call_id: Some(format!("call-{sequence}")),
                tool_success,
                reasoning_text: None,
                tool_requests_json: None,
                tool_arguments_json: None,
            }),
        }
    }

    #[test]
    fn detects_failed_tool_calls_from_structured_metadata() {
        let turns = vec![
            tool_turn(0, Some(true)),
            tool_turn(1, Some(false)),
            tool_turn(2, None),
        ];

        let signals = mine_structured_feedback_signals(&turns);

        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].kind, FeedbackSignalKind::FailedToolCall);
        assert_eq!(signals[0].sequence, Some(1));
        assert_eq!(signals[0].tool_call_id.as_deref(), Some("call-1"));
    }

    #[test]
    fn ignores_message_text_and_non_tool_roles() {
        // A turn that would have tripped the old confusion-marker heuristic
        // ("error", "conflict", "wrong") must produce no signal now, because
        // message content is no longer inspected.
        let mut assistant = tool_turn(0, Some(false));
        assistant.role = SessionRole::Assistant;
        assistant.content =
            "This failed with a conflict and the wrong error".to_string();

        let signals = mine_structured_feedback_signals(&[assistant]);

        assert!(signals.is_empty());
    }

    fn feedback_ingest_result_event(
        tool_success: Option<bool>,
        arguments: Value,
    ) -> CopilotHookEvent {
        CopilotHookEvent {
            event_id: Some("evt-ingest-1".to_string()),
            parent_event_id: None,
            event_type: Some("tool.execution_result".to_string()),
            captured_at: Some(Utc::now()),
            turn_id: None,
            message_id: None,
            tool_call_id: Some("call-ingest-1".to_string()),
            // Real captured transcripts prefix the tool name with a
            // session-scoped client alias (e.g. `mcp_rmcp5_feedback_ingest`);
            // matching is done on the suffix, not an exact literal.
            tool_name: Some("mcp_rmcp5_feedback_ingest".to_string()),
            tool_success,
            reasoning_text: None,
            tool_requests_json: None,
            tool_arguments_json: Some(arguments),
            data_json: None,
            raw_event_json: None,
        }
    }

    fn ingest_arguments() -> Value {
        serde_json::json!({
            "workspace": "c:/repo/memory-api",
            "workspace_slug": "memory-api",
            "source": "agent",
            "target": "ce://memory-api/rule/some-rule",
            "rating": "not-helpful",
            "note": "confusing wording",
            "note_kind": "note",
            "session_id": "session-ingest-1",
            "author": "copilot-gpt5"
        })
    }

    #[test]
    fn detects_explicit_ingestion_tool_call_from_events() {
        let event =
            feedback_ingest_result_event(Some(false), ingest_arguments());

        let signals = mine_explicit_ingestion_signals(&[event]);

        assert_eq!(signals.len(), 1);
        let signal = &signals[0];
        assert_eq!(signal.kind, FeedbackSignalKind::ExplicitIngestion);
        assert_eq!(signal.sequence, None);
        assert_eq!(signal.tool_call_id.as_deref(), Some("call-ingest-1"));
        assert_eq!(signal.tool_success, Some(false));
        let ingestion = signal.ingestion.as_ref().expect("ingestion payload");
        assert_eq!(
            ingestion.target.as_deref(),
            Some("ce://memory-api/rule/some-rule")
        );
        assert_eq!(ingestion.rating.as_deref(), Some("not-helpful"));
        assert_eq!(ingestion.session_id.as_deref(), Some("session-ingest-1"));
    }

    #[test]
    fn detects_explicit_ingestion_from_execution_complete_event() {
        let mut event =
            feedback_ingest_result_event(Some(false), ingest_arguments());
        event.event_type = Some("tool.execution_complete".to_string());

        let signals = mine_explicit_ingestion_signals(&[event]);

        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].kind, FeedbackSignalKind::ExplicitIngestion);
    }

    #[test]
    fn deduplicates_explicit_ingestion_when_complete_and_result_overlap() {
        let mut complete =
            feedback_ingest_result_event(Some(false), ingest_arguments());
        complete.event_type = Some("tool.execution_complete".to_string());
        let mut result = complete.clone();
        result.event_id = Some("evt-ingest-2".to_string());
        result.event_type = Some("tool.execution_result".to_string());

        let signals = mine_explicit_ingestion_signals(&[complete, result]);

        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].kind, FeedbackSignalKind::ExplicitIngestion);
    }

    #[test]
    fn ignores_non_ingest_tool_calls_and_non_result_events() {
        let mut other_tool =
            feedback_ingest_result_event(Some(true), ingest_arguments());
        other_tool.tool_name = Some("mcp_rmcp5_feedback_inbox".to_string());

        let mut wrong_event_type =
            feedback_ingest_result_event(Some(false), ingest_arguments());
        wrong_event_type.event_type = Some("tool.execution_start".to_string());

        let signals =
            mine_explicit_ingestion_signals(&[other_tool, wrong_event_type]);

        assert!(signals.is_empty());
    }

    #[test]
    fn recovers_feedback_entry_for_failed_ingestion_call() {
        let event =
            feedback_ingest_result_event(Some(false), ingest_arguments());
        let signals = mine_explicit_ingestion_signals(&[event]);

        let entry = recover_feedback_entry_from_signal(&signals[0], None)
            .unwrap()
            .expect("recovered entry");

        assert_eq!(entry.target.to_string(), "ce://memory-api/rule/some-rule");
        assert_eq!(entry.rating, Some(FeedbackRating::NotHelpful));
        assert_eq!(entry.note_text.as_deref(), Some("confusing wording"));
        assert_eq!(
            entry.provenance.session_id.as_deref(),
            Some("session-ingest-1")
        );
        assert_eq!(
            entry.provenance.tool_call_id.as_deref(),
            Some("call-ingest-1")
        );
        assert_eq!(entry.provenance.turn_sequence, None);
    }

    #[test]
    fn does_not_duplicate_a_successfully_persisted_ingestion_call() {
        let event =
            feedback_ingest_result_event(Some(true), ingest_arguments());
        let signals = mine_explicit_ingestion_signals(&[event]);

        // tool_success == Some(true) means the live call already persisted
        // its own entry; recovering it again here would double-record it.
        let recovered =
            recover_feedback_entry_from_signal(&signals[0], None).unwrap();
        assert!(recovered.is_none());
    }

    #[test]
    fn skips_recovery_when_required_arguments_are_missing() {
        let mut arguments = ingest_arguments();
        arguments.as_object_mut().unwrap().remove("target");
        let event = feedback_ingest_result_event(Some(false), arguments);
        let signals = mine_explicit_ingestion_signals(&[event]);

        let recovered =
            recover_feedback_entry_from_signal(&signals[0], None).unwrap();
        assert!(recovered.is_none());
    }

    fn failed_tool_call_event(
        tool_name: Option<&str>,
        arguments: Value,
    ) -> CopilotHookEvent {
        CopilotHookEvent {
            event_id: Some("evt-fail-1".to_string()),
            parent_event_id: None,
            event_type: Some("tool.execution_result".to_string()),
            captured_at: Some(Utc::now()),
            turn_id: None,
            message_id: None,
            tool_call_id: Some("call-fail-1".to_string()),
            tool_name: tool_name.map(str::to_string),
            tool_success: Some(false),
            reasoning_text: None,
            tool_requests_json: None,
            tool_arguments_json: Some(arguments),
            data_json: None,
            raw_event_json: None,
        }
    }

    #[test]
    fn maps_ticket_id_keyed_method_to_ticket_entity() {
        // Grounded against the most common real failure: `board_check_out`
        // (23 of 115 failed calls in the committed session store) takes
        // `ticket_id`.
        let mapping = map_failed_tool_call_to_entity(
            Some("mcp_rmcp6_board_check_out"),
            Some(&serde_json::json!({ "ticket_id": "abc123" })),
            "memory-api",
        );

        assert_eq!(
            mapping,
            FailedToolCallMapping::Entity {
                urn: EntityUrn::ticket("memory-api", "abc123").unwrap(),
            }
        );
    }

    #[test]
    fn detects_failed_call_from_execution_complete_event() {
        let mut event = failed_tool_call_event(
            Some("mcp_rmcp6_board_check_out"),
            serde_json::json!({ "ticket_id": "abc123" }),
        );
        event.event_type = Some("tool.execution_complete".to_string());

        let signals = mine_failed_tool_call_signals(&[event], "memory-api");

        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].kind, FeedbackSignalKind::FailedToolCall);
    }

    #[test]
    fn deduplicates_failed_call_when_complete_and_result_overlap() {
        let mut complete = failed_tool_call_event(
            Some("mcp_rmcp6_board_check_out"),
            serde_json::json!({ "ticket_id": "abc123" }),
        );
        complete.event_type = Some("tool.execution_complete".to_string());
        let mut result = complete.clone();
        result.event_id = Some("evt-fail-2".to_string());
        result.event_type = Some("tool.execution_result".to_string());

        let signals = mine_failed_tool_call_signals(&[complete, result], "memory-api");

        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].kind, FeedbackSignalKind::FailedToolCall);
    }

    #[test]
    fn maps_id_keyed_method_to_ticket_entity() {
        // `get_ticket` (and friends) key their ticket reference as `id`.
        let mapping = map_failed_tool_call_to_entity(
            Some("mcp_rmcp6_get_ticket"),
            Some(&serde_json::json!({ "id": "abc123" })),
            "memory-api",
        );

        assert_eq!(
            mapping,
            FailedToolCallMapping::Entity {
                urn: EntityUrn::ticket("memory-api", "abc123").unwrap(),
            }
        );
    }

    #[test]
    fn create_ticket_failure_is_unmapped_no_entity_id_argument() {
        // create_ticket creates a *new* entity; there is no existing entity
        // to reference.
        let mapping = map_failed_tool_call_to_entity(
            Some("mcp_rmcp6_create_ticket"),
            Some(&serde_json::json!({ "title": "x" })),
            "memory-api",
        );

        assert_eq!(
            mapping,
            FailedToolCallMapping::Unmapped {
                reason: UnmappedReason::NoEntityIdArgument,
            }
        );
    }

    #[test]
    fn file_tool_failure_is_unmapped_no_supported_entity_store() {
        let mapping = map_failed_tool_call_to_entity(
            Some("read_file"),
            Some(&serde_json::json!({ "filePath": "src/lib.rs" })),
            "memory-api",
        );

        assert_eq!(
            mapping,
            FailedToolCallMapping::Unmapped {
                reason: UnmappedReason::NoSupportedEntityStore,
            }
        );
    }

    #[test]
    fn edge_tool_failure_is_unmapped_ambiguous() {
        let mapping = map_failed_tool_call_to_entity(
            Some("mcp_rmcp6_add_edge"),
            Some(&serde_json::json!({ "from": "a", "to": "b" })),
            "memory-api",
        );

        assert_eq!(
            mapping,
            FailedToolCallMapping::Unmapped {
                reason: UnmappedReason::AmbiguousMultipleCandidates,
            }
        );
    }

    #[test]
    fn unknown_tool_failure_is_unmapped_unknown_tool() {
        let mapping = map_failed_tool_call_to_entity(None, None, "memory-api");

        assert_eq!(
            mapping,
            FailedToolCallMapping::Unmapped {
                reason: UnmappedReason::UnknownTool,
            }
        );
    }

    #[test]
    fn known_ticket_method_missing_id_argument_is_unmapped() {
        // The tool is known, but this particular call's arguments happen not
        // to carry the id — must not guess a target.
        let mapping = map_failed_tool_call_to_entity(
            Some("mcp_rmcp6_get_ticket"),
            Some(&serde_json::json!({ "workspace": "x" })),
            "memory-api",
        );

        assert_eq!(
            mapping,
            FailedToolCallMapping::Unmapped {
                reason: UnmappedReason::NoEntityIdArgument,
            }
        );
    }

    #[test]
    fn mines_failed_tool_call_signal_with_resolved_mapping_from_events() {
        let event = failed_tool_call_event(
            Some("mcp_rmcp6_board_check_out"),
            serde_json::json!({ "ticket_id": "abc123" }),
        );

        let signals = mine_failed_tool_call_signals(&[event], "memory-api");

        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].kind, FeedbackSignalKind::FailedToolCall);
        assert_eq!(signals[0].sequence, None);
        assert_eq!(
            signals[0].mapping,
            Some(FailedToolCallMapping::Entity {
                urn: EntityUrn::ticket("memory-api", "abc123").unwrap(),
            })
        );
    }

    #[test]
    fn ignores_successful_calls_when_mining_failed_tool_call_signals() {
        let mut event = failed_tool_call_event(
            Some("mcp_rmcp6_board_check_out"),
            serde_json::json!({ "ticket_id": "abc123" }),
        );
        event.tool_success = Some(true);

        let signals = mine_failed_tool_call_signals(&[event], "memory-api");

        assert!(signals.is_empty());
    }

    fn failed_tool_call_signal(urn: EntityUrn) -> StructuredFeedbackSignal {
        StructuredFeedbackSignal {
            kind: FeedbackSignalKind::FailedToolCall,
            sequence: None,
            tool_name: Some("mcp_rmcp6_board_check_out".to_string()),
            tool_call_id: None,
            event_id: None,
            tool_success: Some(false),
            ingestion: None,
            mapping: Some(FailedToolCallMapping::Entity { urn }),
        }
    }

    fn unmapped_failed_tool_call_signal() -> StructuredFeedbackSignal {
        StructuredFeedbackSignal {
            kind: FeedbackSignalKind::FailedToolCall,
            sequence: None,
            tool_name: Some("read_file".to_string()),
            tool_call_id: None,
            event_id: None,
            tool_success: Some(false),
            ingestion: None,
            mapping: Some(FailedToolCallMapping::Unmapped {
                reason: UnmappedReason::NoSupportedEntityStore,
            }),
        }
    }

    fn explicit_ingestion_signal(target: &str) -> StructuredFeedbackSignal {
        StructuredFeedbackSignal {
            kind: FeedbackSignalKind::ExplicitIngestion,
            sequence: None,
            tool_name: Some("mcp_rmcp5_feedback_ingest".to_string()),
            tool_call_id: None,
            event_id: None,
            tool_success: Some(true),
            ingestion: Some(ExplicitIngestionArgs {
                target: Some(target.to_string()),
                source: Some("agent".to_string()),
                rating: None,
                note: None,
                note_kind: None,
                session_id: None,
                author: None,
            }),
            mapping: None,
        }
    }

    #[test]
    fn discovers_entities_in_first_seen_order_deduped_across_a_session() {
        let t1 = EntityUrn::ticket("memory-api", "t1").unwrap();
        let t2 = EntityUrn::ticket("memory-api", "t2").unwrap();

        // A multi-entity session: t1 fails, a rule is explicitly rated, t1
        // fails again (duplicate), an unmapped file-tool failure (no
        // entity), t2 is explicitly rated, then t1 fails a third time.
        let signals = vec![
            failed_tool_call_signal(t1.clone()),
            explicit_ingestion_signal("ce://memory-api/rule/r1"),
            failed_tool_call_signal(t1.clone()),
            unmapped_failed_tool_call_signal(),
            explicit_ingestion_signal("ce://memory-api/ticket/t2"),
            failed_tool_call_signal(t1.clone()),
        ];

        let discovered = discover_entities_from_signals(&signals);

        assert_eq!(
            discovered,
            vec![t1, EntityUrn::rule("memory-api", "r1").unwrap(), t2,]
        );
    }

    #[test]
    fn entity_discovery_queue_dedupes_repeated_enqueues() {
        let mut queue = EntityDiscoveryQueue::new();
        let urn = EntityUrn::ticket("memory-api", "t1").unwrap();

        assert!(queue.enqueue(urn.clone()));
        assert!(!queue.enqueue(urn.clone()));
        assert!(!queue.enqueue(urn.clone()));

        assert_eq!(queue.into_ordered(), vec![urn]);
    }
}
