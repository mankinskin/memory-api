use std::{
    collections::HashMap,
    fs::File,
    io::{
        BufRead,
        BufReader,
    },
    path::Path,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;

use crate::{
    SessionError,
    SessionLinks,
    SessionMetadata,
    SessionRecord,
    SessionRole,
    SessionTurn,
    SessionTurnEventMeta,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotHookEvent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_success: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_requests_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_arguments_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_event_json: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotRuntimeMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copilot_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vscode_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotHookMessage {
    pub role: SessionRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_meta: Option<SessionTurnEventMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotHookPayload {
    pub session_id: String,
    pub workspace_slug: String,
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(default)]
    pub messages: Vec<CopilotHookMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<CopilotHookEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<CopilotRuntimeMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCaptureRequest {
    pub source: String,
    pub payload: CopilotHookPayload,
    #[serde(default)]
    pub links: SessionLinks,
}

impl SessionCaptureRequest {
    pub fn copilot(payload: CopilotHookPayload) -> Self {
        Self {
            source: "copilot-hook".to_string(),
            payload,
            links: SessionLinks::default(),
        }
    }

    pub fn into_record(self) -> Result<SessionRecord, SessionError> {
        self.into_record_and_events().map(|(record, _)| record)
    }

    pub fn into_record_and_events(
        self
    ) -> Result<(SessionRecord, Vec<CopilotHookEvent>), SessionError> {
        let payload = self.payload;
        if payload.session_id.trim().is_empty() {
            return Err(SessionError::MissingSessionId);
        }
        if payload.messages.is_empty() {
            return Err(SessionError::EmptyTurns);
        }

        let captured_at = payload.captured_at;
        let turns: Vec<SessionTurn> = payload
            .messages
            .into_iter()
            .enumerate()
            .map(|(sequence, message)| SessionTurn {
                sequence,
                role: message.role,
                content: message.content,
                captured_at: message.captured_at.unwrap_or(captured_at),
                tool_name: message.tool_name,
                event_meta: message.event_meta,
            })
            .collect();
        let started_at = turns
            .first()
            .map(|turn| turn.captured_at)
            .unwrap_or(captured_at);

        let runtime = payload.runtime.unwrap_or(CopilotRuntimeMetadata {
            producer: None,
            copilot_version: None,
            vscode_version: None,
            protocol_version: None,
        });

        Ok((
            SessionRecord {
                schema_version: crate::SESSION_SCHEMA_VERSION,
                session_id: payload.session_id,
                source: self.source,
                started_at,
                captured_at,
                metadata: SessionMetadata {
                    workspace_slug: payload.workspace_slug,
                    conversation_id: payload.conversation_id,
                    agent_id: payload.agent_id,
                    ticket_id: None,
                    model: payload.model,
                    trigger: payload.trigger,
                    producer: runtime.producer,
                    copilot_version: runtime.copilot_version,
                    vscode_version: runtime.vscode_version,
                    protocol_version: runtime.protocol_version,
                    worktree: None,
                },
                turns,
                links: self.links,
            },
            payload.events,
        ))
    }
}

impl TryFrom<SessionCaptureRequest> for SessionRecord {
    type Error = SessionError;

    fn try_from(value: SessionCaptureRequest) -> Result<Self, Self::Error> {
        value.into_record()
    }
}

#[derive(Debug)]
struct TranscriptEventEnvelope {
    event_id: Option<String>,
    parent_event_id: Option<String>,
    event_type: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    data: serde_json::Value,
    role_hint: Option<SessionRole>,
    content_hint: Option<String>,
    turn_id: Option<String>,
    message_id: Option<String>,
    tool_call_id: Option<String>,
    tool_name: Option<String>,
    tool_success: Option<bool>,
    reasoning_text: Option<String>,
    tool_requests_json: Option<Value>,
    tool_arguments_json: Option<Value>,
    data_json: Option<Value>,
    raw_event_json: Option<Value>,
}

#[derive(Debug, Clone)]
struct ToolExecutionContext {
    started_at: Option<DateTime<Utc>>,
    tool_name: Option<String>,
    tool_arguments_json: Option<Value>,
}

impl TranscriptEventEnvelope {
    fn event_meta(&self) -> Option<SessionTurnEventMeta> {
        let meta = SessionTurnEventMeta {
            event_id: self.event_id.clone(),
            parent_event_id: self.parent_event_id.clone(),
            event_type: self.event_type.clone(),
            turn_id: self.turn_id.clone(),
            message_id: self.message_id.clone(),
            tool_call_id: self.tool_call_id.clone(),
            tool_success: self.tool_success,
            reasoning_text: self.reasoning_text.clone(),
            tool_requests_json: self.tool_requests_json.clone(),
            tool_arguments_json: self.tool_arguments_json.clone(),
        };
        if meta.event_id.is_none()
            && meta.parent_event_id.is_none()
            && meta.event_type.is_none()
            && meta.turn_id.is_none()
            && meta.message_id.is_none()
            && meta.tool_call_id.is_none()
            && meta.tool_success.is_none()
            && meta.reasoning_text.is_none()
            && meta.tool_requests_json.is_none()
            && meta.tool_arguments_json.is_none()
        {
            None
        } else {
            Some(meta)
        }
    }

    fn captured_event(&self) -> CopilotHookEvent {
        CopilotHookEvent {
            event_id: self.event_id.clone(),
            parent_event_id: self.parent_event_id.clone(),
            event_type: self.event_type.clone(),
            captured_at: self.timestamp,
            turn_id: self.turn_id.clone(),
            message_id: self.message_id.clone(),
            tool_call_id: self.tool_call_id.clone(),
            tool_name: self.tool_name.clone(),
            tool_success: self.tool_success,
            reasoning_text: self.reasoning_text.clone(),
            tool_requests_json: self.tool_requests_json.clone(),
            tool_arguments_json: self.tool_arguments_json.clone(),
            data_json: self.data_json.clone(),
            raw_event_json: self.raw_event_json.clone(),
        }
    }
}

pub fn copilot_payload_from_transcript_path(
    transcript_path: impl AsRef<Path>,
    workspace_slug: impl Into<String>,
    trigger: Option<String>,
) -> Result<CopilotHookPayload, SessionError> {
    let transcript_path = transcript_path.as_ref();
    let file =
        File::open(transcript_path).map_err(|source| SessionError::Io {
            path: transcript_path.to_path_buf(),
            source,
        })?;
    let reader = BufReader::new(file);

    copilot_payload_from_transcript_reader_with_path(
        reader,
        transcript_path,
        workspace_slug.into(),
        trigger,
    )
}

pub fn copilot_payload_from_transcript_reader<R: BufRead>(
    reader: R,
    workspace_slug: impl Into<String>,
    trigger: Option<String>,
) -> Result<CopilotHookPayload, SessionError> {
    copilot_payload_from_transcript_reader_with_path(
        reader,
        Path::new("<copilot-transcript>"),
        workspace_slug.into(),
        trigger,
    )
}

fn copilot_payload_from_transcript_reader_with_path<R: BufRead>(
    reader: R,
    transcript_path: &Path,
    workspace_slug: String,
    trigger: Option<String>,
) -> Result<CopilotHookPayload, SessionError> {
    let mut session_id = None;
    let mut agent_id = None;
    let mut captured_at = None;
    let mut started_at = None;
    let mut runtime = CopilotRuntimeMetadata {
        producer: None,
        copilot_version: None,
        vscode_version: None,
        protocol_version: None,
    };
    let mut messages = vec![];
    let mut events = vec![];
    let mut tool_execution_contexts: HashMap<String, ToolExecutionContext> =
        HashMap::new();

    for line in reader.lines() {
        let line = line.map_err(|source| SessionError::Io {
            path: transcript_path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }

        let mut event = deserialize_transcript_event(&line, transcript_path)?;
        retag_tool_only_assistant_message(&mut event);

        if let Some(context) = capture_tool_execution_context(&event) {
            if let Some(tool_call_id) = event.tool_call_id.clone() {
                tool_execution_contexts.insert(tool_call_id, context);
            }
        }

        let tool_call_key = event.tool_call_id.clone().unwrap_or_default();
        let context = tool_execution_contexts.get(tool_call_key.as_str());
        hydrate_tool_execution_complete(&mut event, context);

        let captured_event = event.captured_event();
        events.push(captured_event);

        if let Some(result_event) = build_tool_execution_result_event(
            &event,
            context,
        ) {
            events.push(result_event);
        }

        match event.event_type.as_deref() {
            Some("session.start")
            | Some("session_start")
            | Some("sessionStart") => handle_session_start_event(
                &event,
                &mut session_id,
                &mut agent_id,
                &mut started_at,
                &mut captured_at,
                &mut runtime,
            )?,
            Some("user.message") | Some("user_message") =>
                handle_message_event(
                    &event,
                    SessionRole::User,
                    &mut captured_at,
                    &mut messages,
                )?,
            Some("assistant.message") | Some("assistant_message") =>
                handle_message_event(
                    &event,
                    SessionRole::Assistant,
                    &mut captured_at,
                    &mut messages,
                )?,
            _ =>
                if let Some(role) = event.role_hint.clone() {
                    handle_message_event(
                        &event,
                        role,
                        &mut captured_at,
                        &mut messages,
                    )?;
                },
        }
    }

    let session_id = session_id.ok_or(SessionError::MissingSessionId)?;
    if messages.is_empty() {
        return Err(SessionError::EmptyTurns);
    }

    let runtime = if runtime.producer.is_none()
        && runtime.copilot_version.is_none()
        && runtime.vscode_version.is_none()
        && runtime.protocol_version.is_none()
    {
        None
    } else {
        Some(runtime)
    };

    Ok(CopilotHookPayload {
        session_id,
        workspace_slug,
        captured_at: captured_at.or(started_at).unwrap_or_else(Utc::now),
        conversation_id: None,
        agent_id,
        model: None,
        trigger,
        messages,
        events,
        runtime,
    })
}

fn handle_session_start_event(
    event: &TranscriptEventEnvelope,
    session_id: &mut Option<String>,
    agent_id: &mut Option<String>,
    started_at: &mut Option<DateTime<Utc>>,
    captured_at: &mut Option<DateTime<Utc>>,
    runtime: &mut CopilotRuntimeMetadata,
) -> Result<(), SessionError> {
    let data = &event.data;

    let session_id_value =
        json_string(data, &["sessionId", "session_id", "id"]);
    let producer_value =
        json_string(data, &["producer", "agentId", "agent_id"]);
    let start_time_value = json_timestamp(data, &["startTime", "start_time"]);

    if session_id.is_none() {
        *session_id = session_id_value;
    }
    if agent_id.is_none() {
        *agent_id = producer_value.clone();
    }
    if started_at.is_none() {
        *started_at = start_time_value.or(event.timestamp);
    }
    if captured_at.is_none() {
        *captured_at = event.timestamp;
    }

    if runtime.producer.is_none() {
        runtime.producer = producer_value;
    }
    if runtime.copilot_version.is_none() {
        runtime.copilot_version =
            json_string(data, &["copilotVersion", "copilot_version"]);
    }
    if runtime.vscode_version.is_none() {
        runtime.vscode_version =
            json_string(data, &["vscodeVersion", "vscode_version"]);
    }
    if runtime.protocol_version.is_none() {
        runtime.protocol_version = data
            .get("version")
            .or_else(|| data.get("protocolVersion"))
            .and_then(serde_json::Value::as_i64);
    }

    Ok(())
}

fn handle_message_event(
    event: &TranscriptEventEnvelope,
    role: SessionRole,
    captured_at: &mut Option<DateTime<Utc>>,
    messages: &mut Vec<CopilotHookMessage>,
) -> Result<(), SessionError> {
    let content = event.content_hint.clone().unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(());
    }

    let timestamp = event.timestamp.unwrap_or_else(Utc::now);
    *captured_at = Some(timestamp);
    messages.push(CopilotHookMessage {
        role,
        content,
        tool_name: event.tool_name.clone(),
        captured_at: Some(timestamp),
        event_meta: event.event_meta(),
    });
    Ok(())
}

fn retag_tool_only_assistant_message(event: &mut TranscriptEventEnvelope) {
    let is_assistant_message = matches!(
        event.event_type.as_deref(),
        Some("assistant.message") | Some("assistant_message")
    );
    if !is_assistant_message {
        return;
    }

    let has_content = event
        .content_hint
        .as_ref()
        .map(|content| !content.trim().is_empty())
        .unwrap_or(false);
    if has_content {
        return;
    }

    let has_tool_requests = event
        .tool_requests_json
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if has_tool_requests {
        event.event_type = Some("assistant.tool_plan".to_string());
    }
}

fn capture_tool_execution_context(
    event: &TranscriptEventEnvelope
) -> Option<ToolExecutionContext> {
    let is_start = matches!(
        event.event_type.as_deref(),
        Some("tool.execution_start") | Some("tool_execution_start")
    );
    if !is_start {
        return None;
    }
    event.tool_call_id.as_ref()?;

    Some(ToolExecutionContext {
        started_at: event.timestamp,
        tool_name: event.tool_name.clone(),
        tool_arguments_json: event.tool_arguments_json.clone(),
    })
}

fn hydrate_tool_execution_complete(
    event: &mut TranscriptEventEnvelope,
    context: Option<&ToolExecutionContext>,
) {
    let is_complete = matches!(
        event.event_type.as_deref(),
        Some("tool.execution_complete") | Some("tool_execution_complete")
    );
    if !is_complete {
        return;
    }

    if event.tool_name.is_none() {
        event.tool_name = context.and_then(|ctx| ctx.tool_name.clone());
    }
    if event.tool_arguments_json.is_none() {
        event.tool_arguments_json =
            context.and_then(|ctx| ctx.tool_arguments_json.clone());
    }
}

fn build_tool_execution_result_event(
    event: &TranscriptEventEnvelope,
    context: Option<&ToolExecutionContext>,
) -> Option<CopilotHookEvent> {
    let is_complete = matches!(
        event.event_type.as_deref(),
        Some("tool.execution_complete") | Some("tool_execution_complete")
    );
    if !is_complete {
        return None;
    }

    let tool_call_id = event.tool_call_id.clone()?;
    let tool_name = event
        .tool_name
        .clone()
        .or_else(|| context.and_then(|ctx| ctx.tool_name.clone()));
    let tool_arguments = event
        .tool_arguments_json
        .clone()
        .or_else(|| context.and_then(|ctx| ctx.tool_arguments_json.clone()));
    let duration_ms = event
        .data
        .get("durationMs")
        .or_else(|| event.data.get("duration_ms"))
        .and_then(serde_json::Value::as_i64)
        .or_else(|| {
            context.and_then(|ctx| {
                let started_at = ctx.started_at?;
                let finished_at = event.timestamp?;
                Some((finished_at - started_at).num_milliseconds())
            })
        });
    let success = event.tool_success;
    let result_code = match success {
        Some(true) => "ok",
        Some(false) => "error",
        None => "unknown",
    };

    let error_type = event
        .data
        .get("error")
        .and_then(serde_json::Value::as_object)
        .and_then(|error| {
            error
                .get("type")
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    error
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                })
        })
        .map(ToString::to_string)
        .or_else(|| {
            event
                .data
                .get("errorType")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        });

    let summary = extract_tool_result_summary(&event.data);
    let spill_pointer = find_spill_pointer(&event.data, summary.as_deref());
    let has_spill = spill_pointer.is_some();

    let sync_terminal_ambiguous = is_sync_terminal_completion_ambiguous(
        tool_name.as_deref(),
        tool_arguments.as_ref(),
        success,
        summary.as_deref(),
        spill_pointer.as_deref(),
        &event.data,
    );

    let mut normalized = serde_json::Map::new();
    normalized.insert(
        "toolCallId".to_string(),
        serde_json::Value::String(tool_call_id.clone()),
    );
    normalized.insert(
        "result_code".to_string(),
        serde_json::Value::String(result_code.to_string()),
    );
    normalized.insert("has_spill".to_string(), serde_json::Value::Bool(has_spill));
    if let Some(name) = tool_name.clone() {
        normalized.insert("tool_name".to_string(), serde_json::Value::String(name));
    }
    if let Some(arguments) = tool_arguments.clone() {
        normalized.insert("arguments".to_string(), arguments);
    }
    if let Some(duration_ms) = duration_ms {
        normalized.insert(
            "duration_ms".to_string(),
            serde_json::Value::Number(duration_ms.into()),
        );
    }
    if let Some(summary) = summary.clone() {
        normalized.insert("summary".to_string(), serde_json::Value::String(summary));
    }
    if let Some(pointer) = spill_pointer.clone() {
        normalized.insert(
            "spill_pointer".to_string(),
            serde_json::Value::String(pointer),
        );
    }
    if let Some(error_type) = error_type {
        normalized.insert(
            "error_type".to_string(),
            serde_json::Value::String(error_type),
        );
    }
    if sync_terminal_ambiguous {
        normalized.insert(
            "blocker".to_string(),
            serde_json::Value::String(
                "sync-terminal-state-ambiguous".to_string(),
            ),
        );
        normalized.insert(
            "lifecycle_state".to_string(),
            serde_json::Value::String("background-ambiguous".to_string()),
        );
        normalized.insert(
            "lifecycle_reason".to_string(),
            serde_json::Value::String(
                "missing-deterministic-sync-completion-metadata".to_string(),
            ),
        );
    }

    Some(CopilotHookEvent {
        event_id: None,
        parent_event_id: event.event_id.clone(),
        event_type: Some("tool.execution_result".to_string()),
        captured_at: event.timestamp,
        turn_id: event.turn_id.clone(),
        message_id: event.message_id.clone(),
        tool_call_id: Some(tool_call_id),
        tool_name,
        tool_success: success,
        reasoning_text: None,
        tool_requests_json: None,
        tool_arguments_json: tool_arguments,
        data_json: Some(serde_json::Value::Object(normalized)),
        raw_event_json: None,
    })
}

fn extract_tool_result_summary(data: &serde_json::Value) -> Option<String> {
    let candidates = ["summary", "output", "stdout", "stderr", "message", "content"];
    let value = candidates
        .iter()
        .filter_map(|key| data.get(*key))
        .find_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())?;

    Some(value.chars().take(240).collect())
}

fn find_spill_pointer(
    data: &serde_json::Value,
    summary: Option<&str>,
) -> Option<String> {
    if let Some(pointer) = data
        .get("spillPointer")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        return Some(pointer.to_string());
    }

    if let Some(pointer) = data
        .get("outputPath")
        .or_else(|| data.get("path"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        return Some(pointer.to_string());
    }

    if let Some(text) = summary {
        if let Some((_, right)) = text.split_once("content at:") {
            let pointer = right.trim();
            if !pointer.is_empty() {
                return Some(pointer.to_string());
            }
        }
        if let Some((_, right)) = text.split_once("saved to:") {
            let pointer = right.trim();
            if !pointer.is_empty() {
                return Some(pointer.to_string());
            }
        }
    }

    None
}

fn is_sync_terminal_completion_ambiguous(
    tool_name: Option<&str>,
    tool_arguments: Option<&serde_json::Value>,
    success: Option<bool>,
    summary: Option<&str>,
    spill_pointer: Option<&str>,
    data: &serde_json::Value,
) -> bool {
    if tool_name != Some("run_in_terminal") || success != Some(true) {
        return false;
    }

    let mode_is_sync = tool_arguments
        .and_then(|arguments| arguments.get("mode"))
        .and_then(serde_json::Value::as_str)
        .map(|mode| mode.eq_ignore_ascii_case("sync"))
        .unwrap_or(false);
    if !mode_is_sync {
        return false;
    }

    // Only flag ambiguity when the completion payload explicitly signals
    // background/timeout/input-needed semantics. A plain sync success event
    // without these signals is treated as deterministic completion.
    if data.get("terminalId").is_some()
        || data.get("terminal_id").is_some()
        || data.get("deferredResultId").is_some()
        || data.get("deferred_result_id").is_some()
    {
        return true;
    }

    if data
        .get("needsInput")
        .or_else(|| data.get("needs_input"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }

    if data
        .get("timedOut")
        .or_else(|| data.get("timed_out"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }

    let text_signals = [
        "moved to background",
        "needs input",
        "waiting for input",
        "timed out",
    ];

    [summary, spill_pointer]
        .into_iter()
        .flatten()
        .map(|text| text.to_ascii_lowercase())
        .any(|text| text_signals.iter().any(|signal| text.contains(signal)))
}

fn deserialize_transcript_event(
    line: &str,
    transcript_path: &Path,
) -> Result<TranscriptEventEnvelope, SessionError> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|source| {
            SessionError::Deserialize {
                path: transcript_path.to_path_buf(),
                source,
            }
        })?;

    let value = normalize_embedded_json_strings(value);

    let data = value.get("data").cloned().unwrap_or_else(|| value.clone());

    let event_id = json_string(&value, &["id"]);
    let parent_event_id = json_string(&value, &["parentId", "parent_id"]);

    let event_type = first_non_empty_string(&[
        value.get("type").and_then(serde_json::Value::as_str),
        value.get("event").and_then(serde_json::Value::as_str),
        value.get("name").and_then(serde_json::Value::as_str),
    ])
    .map(ToString::to_string);

    let timestamp = value
        .get("timestamp")
        .and_then(parse_timestamp_value)
        .or_else(|| value.get("ts").and_then(parse_timestamp_value))
        .or_else(|| data.get("timestamp").and_then(parse_timestamp_value))
        .or_else(|| data.get("ts").and_then(parse_timestamp_value));

    let role_hint = parse_role(
        value
            .get("role")
            .and_then(serde_json::Value::as_str)
            .or_else(|| data.get("role").and_then(serde_json::Value::as_str)),
    );

    let content_hint = first_non_empty_string(&[
        value.get("content").and_then(serde_json::Value::as_str),
        value.get("text").and_then(serde_json::Value::as_str),
        data.get("content").and_then(serde_json::Value::as_str),
        data.get("text").and_then(serde_json::Value::as_str),
    ])
    .map(ToString::to_string);

    let turn_id = json_string(&data, &["turnId", "turn_id"]);
    let message_id = json_string(&data, &["messageId", "message_id"]);
    let tool_call_id = json_string(&data, &["toolCallId", "tool_call_id"]);
    let tool_name = json_string(&data, &["toolName", "tool_name"]);
    let tool_success = data.get("success").and_then(serde_json::Value::as_bool);
    let reasoning_text =
        json_string(&data, &["reasoningText", "reasoning_text"]);
    let tool_requests_json = json_value(&data, "toolRequests");
    let tool_arguments_json = json_value(&data, "arguments");
    let data_json = Some(data.clone());
    let raw_event_json = Some(value.clone());

    Ok(TranscriptEventEnvelope {
        event_id,
        parent_event_id,
        event_type,
        timestamp,
        data,
        role_hint,
        content_hint,
        turn_id,
        message_id,
        tool_call_id,
        tool_name,
        tool_success,
        reasoning_text,
        tool_requests_json,
        tool_arguments_json,
        data_json,
        raw_event_json,
    })
}

fn json_string(
    value: &serde_json::Value,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
}

fn json_timestamp(
    value: &serde_json::Value,
    keys: &[&str],
) -> Option<DateTime<Utc>> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(parse_timestamp_value)
}

fn json_value(
    value: &serde_json::Value,
    key: &str,
) -> Option<Value> {
    value.get(key).cloned()
}

fn normalize_embedded_json_strings(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(normalize_embedded_json_strings)
                .collect(),
        ),
        Value::Object(entries) => Value::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key, normalize_embedded_json_strings(value)))
                .collect(),
        ),
        Value::String(text) =>
            parse_stringified_json_value(&text)
                .map(normalize_embedded_json_strings)
                .unwrap_or(Value::String(text)),
        other => other,
    }
}

fn parse_stringified_json_value(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // De-stringify JSON payloads that were double-encoded upstream.
    let starts_like_json =
        trimmed.starts_with('{') || trimmed.starts_with('[');
    if !starts_like_json {
        return None;
    }

    serde_json::from_str(trimmed).ok()
}

fn parse_timestamp_value(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    if let Some(text) = value.as_str() {
        return DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|timestamp| timestamp.with_timezone(&Utc));
    }
    if let Some(millis) = value.as_i64() {
        return DateTime::<Utc>::from_timestamp_millis(millis);
    }
    None
}

fn parse_role(role: Option<&str>) -> Option<SessionRole> {
    match role?.trim().to_ascii_lowercase().as_str() {
        "user" => Some(SessionRole::User),
        "assistant" | "model" => Some(SessionRole::Assistant),
        "tool" => Some(SessionRole::Tool),
        "system" => Some(SessionRole::System),
        _ => None,
    }
}

fn first_non_empty_string<'a>(values: &[Option<&'a str>]) -> Option<&'a str> {
    values
        .iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "hook/tests.rs"]
mod tests;
