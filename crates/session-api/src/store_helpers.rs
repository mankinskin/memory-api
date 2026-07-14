use super::*;

pub(super) fn write_json<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), SessionError> {
    let parent = path
        .parent()
        .ok_or_else(|| SessionError::InvalidStorePath(path.to_path_buf()))?;
    fs::create_dir_all(parent).map_err(|source| SessionError::Io {
        path: parent.to_path_buf(),
        source,
    })?;

    let encoded = serde_json::to_vec_pretty(value).map_err(|source| {
        SessionError::Serialize {
            path: path.to_path_buf(),
            source,
        }
    })?;

    let tmp_path = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("session"),
        uuid::Uuid::new_v4()
    ));

    {
        let mut file =
            fs::File::create(&tmp_path).map_err(|source| SessionError::Io {
                path: tmp_path.clone(),
                source,
            })?;
        use std::io::Write;
        file.write_all(&encoded)
            .map_err(|source| SessionError::Io {
                path: tmp_path.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| SessionError::Io {
            path: tmp_path.clone(),
            source,
        })?;
    }

    // `std::fs::rename` is an atomic replace on every platform this store runs on:
    // on Unix it is `rename(2)`, and on Windows it maps to `MoveFileExW` with
    // `MOVEFILE_REPLACE_EXISTING`, which atomically swaps the destination inode.
    // A single rename therefore has no crash window — the destination path always
    // resolves to either the old durable file or the fully-written new one, never
    // to a missing file. (An earlier Windows-only "move aside to a backup, then
    // promote the temp" dance was removed: it wrongly assumed Windows rename cannot
    // overwrite, and it opened a crash interval during which the destination was
    // absent and the previous state survived only under an unreferenced backup.)
    fs::rename(&tmp_path, path).map_err(|source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    if let Ok(parent_dir) = fs::File::open(parent) {
        let _ = parent_dir.sync_all();
    }

    Ok(())
}

pub(super) fn read_json<T: DeserializeOwned>(
    path: &Path
) -> Result<T, SessionError> {
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

pub(super) fn read_json_if_exists<T: DeserializeOwned>(
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

pub(super) fn merge_manifest(
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

pub(super) fn merge_metadata(
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

pub(super) fn validate_worktree_request(
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

pub(super) fn ensure_supported_schema_version(
    path: &Path,
    found: u32,
) -> Result<(), SessionError> {
    if found == SESSION_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(SessionError::SchemaVersionMismatch {
            path: path.to_path_buf(),
            found,
            expected: SESSION_SCHEMA_VERSION,
        })
    }
}

pub(super) fn can_reuse_assignment(
    existing: &SessionWorktreeAssignment,
    request: &SessionWorktreeCheckInRequest,
) -> bool {
    existing.status == SessionWorktreeStatus::Active
        && existing.path == request.worktree_path
        && existing.branch == request.branch
        && existing.path.exists()
}

pub(super) fn receipt_from_record(
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

pub(super) fn merge_links(
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

pub(super) fn extend_unique(
    target: &mut Vec<String>,
    incoming: Vec<String>,
) {
    for value in incoming {
        if !target.iter().any(|existing| existing == &value) {
            target.push(value);
        }
    }
}

pub(super) fn merge_events(
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

pub(super) fn captured_event_key(event: &CopilotHookEvent) -> String {
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
        json_fingerprint(&event.tool_requests_json),
        json_fingerprint(&event.tool_arguments_json),
        json_fingerprint(&event.data_json),
        json_fingerprint(&event.raw_event_json),
    )
}

fn json_fingerprint(value: &Option<serde_json::Value>) -> String {
    value
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok())
        .unwrap_or_default()
}

pub(super) fn merge_transcript(
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
                // Hook captures are periodic snapshots; when histories diverge,
                // keep the newest complete snapshot instead of rejecting sync.
                if incoming.turns.len() >= existing.turns.len() {
                    return Ok(incoming);
                }
                return Ok(existing);
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

pub(super) fn turns_match(
    left: &SessionTurn,
    right: &SessionTurn,
) -> bool {
    left.sequence == right.sequence
        && left.role == right.role
        && left.content == right.content
        && left.tool_name == right.tool_name
        && left.event_meta == right.event_meta
}

pub(super) fn session_matches_query(
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

pub(super) fn validate_segment(
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
