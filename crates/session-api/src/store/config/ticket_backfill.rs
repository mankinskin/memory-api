impl SessionStoreConfig {
    /// Backfill ticket linkage for historical sessions using ONLY
    /// structured signals: `branch` shape, `worktree_path` shape, and
    /// handoff-package `target_tickets`. Never reads `transcript.json` text
    /// (spec e5f8a2c1 forbids transcript scanning for linkage at every
    /// tier). Idempotent: an already-populated `metadata.ticket_id` is never
    /// overwritten, and a ticket id already present in `links.ticket_ids` is
    /// never duplicated. When `write` is `false` this only computes the
    /// report; no session file is touched.
    pub fn backfill_ticket_links(
        &self,
        write: bool,
    ) -> Result<SessionTicketBackfillReport, SessionError> {
        let sessions_root = self.sessions_root()?;
        let mut report = SessionTicketBackfillReport::default();
        if !sessions_root.exists() {
            return Ok(report);
        }

        let ticket_store_root = self.ticket_store_root();
        let ticket_store = if ticket_store_root.exists() {
            Some(
                TicketStore::open(&ticket_store_root).map_err(|error| {
                    SessionError::InvalidHookInput(format!(
                        "ticket store unavailable at {}: {error}",
                        ticket_store_root.display()
                    ))
                })?,
            )
        } else {
            None
        };

        for entry in
            fs::read_dir(&sessions_root).map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?
        {
            let entry = entry.map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?;
            let file_type =
                entry.file_type().map_err(|source| SessionError::Io {
                    path: entry.path(),
                    source,
                })?;
            if !file_type.is_dir() {
                continue;
            }

            report.total_sessions += 1;
            let session_id = entry.file_name().to_string_lossy().into_owned();
            let mut record = match self.read_session(&session_id) {
                Ok(record) => record,
                Err(_) => {
                    report.skipped_corrupt += 1;
                    continue;
                },
            };

            let mut changed = false;

            if record.metadata.ticket_id.is_some() {
                report.already_linked_untouched += 1;
            } else if let Some(worktree) = record.metadata.worktree.clone() {
                let short_id = parse_agent_branch_short_id(&worktree.branch)
                    .or_else(|| parse_worktree_path_short_id(&worktree.path));
                if let Some(short_id) = short_id {
                    match resolve_ticket_prefix(
                        ticket_store.as_ref(),
                        &short_id,
                    ) {
                        Some(full_id) => {
                            let via_branch = parse_agent_branch_short_id(
                                &worktree.branch,
                            )
                            .is_some();
                            record.metadata.ticket_id = Some(full_id);
                            if via_branch {
                                report.linked_via_branch += 1;
                            } else {
                                report.linked_via_worktree_path += 1;
                            }
                            changed = true;
                        },
                        None => {
                            report.skipped_unresolvable_shortid += 1;
                        },
                    }
                }
            }

            let handoff_targets =
                self.session_handoff_target_tickets(&entry.path())?;
            if !handoff_targets.is_empty() {
                report.handoff_already_at_mentioned = true;
            }
            for target in handoff_targets {
                if record.links.links_to_ticket(&target) {
                    report.already_linked_untouched += 1;
                    continue;
                }
                match resolve_ticket_prefix(ticket_store.as_ref(), &target) {
                    Some(full_id) => {
                        if !record.links.links_to_ticket(&full_id) {
                            record.links.ticket_ids.push(full_id);
                            report.linked_via_handoff += 1;
                            changed = true;
                        }
                    },
                    None => {
                        report.skipped_unresolvable_shortid += 1;
                    },
                }
            }

            if changed {
                report.total_would_link += 1;
                if write {
                    self.persist_record(record)?;
                }
            }
        }

        Ok(report)
    }

    /// Collect the deduplicated union of `target_tickets` across every
    /// `handoffs/*/handoff.json` on disk for one session. Structured-data
    /// read only, same source `sessions_for_ticket`'s mentioned tier already
    /// scans; never touches `transcript.json`.
    fn session_handoff_target_tickets(
        &self,
        session_dir: &Path,
    ) -> Result<BTreeSet<String>, SessionError> {
        let handoffs_dir = session_dir.join("handoffs");
        let mut targets = BTreeSet::new();
        if !handoffs_dir.exists() {
            return Ok(targets);
        }

        for entry in
            fs::read_dir(&handoffs_dir).map_err(|source| SessionError::Io {
                path: handoffs_dir.clone(),
                source,
            })?
        {
            let entry = entry.map_err(|source| SessionError::Io {
                path: handoffs_dir.clone(),
                source,
            })?;
            let handoff_json_path = entry.path().join("handoff.json");
            if let Some(record) = read_json_if_exists::<SessionHandoffRecord>(
                &handoff_json_path,
            )? {
                targets.extend(record.target_tickets);
            }
        }

        Ok(targets)
    }
}

/// Parses the 8-hex-char short id out of an `agent/<short-id>-<slug>`
/// branch name. Returns `None` for any other shape.
fn parse_agent_branch_short_id(branch: &str) -> Option<String> {
    let rest = branch.strip_prefix("agent/")?;
    short_id_prefix(rest)
}

/// Parses the 8-hex-char short id out of a `.worktrees/<short-id>-<slug>`
/// path component. Returns `None` when no `.worktrees` component is present
/// or the following component does not match the shape.
fn parse_worktree_path_short_id(path: &Path) -> Option<String> {
    let mut components = path.components();
    while let Some(component) = components.next() {
        if component.as_os_str() == ".worktrees" {
            let next = components.next()?;
            let name = next.as_os_str().to_str()?;
            return short_id_prefix(name);
        }
    }
    None
}

/// Shared `<8-hex-chars>-<rest>` shape check used by both the branch and
/// worktree-path parsers.
fn short_id_prefix(candidate: &str) -> Option<String> {
    let bytes = candidate.as_bytes();
    if bytes.len() < 9 || bytes[8] != b'-' {
        return None;
    }
    let prefix = &candidate[..8];
    if prefix.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(prefix.to_ascii_lowercase())
    } else {
        None
    }
}

/// Resolves and verifies `prefix` (short id or full ticket id) against the
/// real ticket store. Returns `None` (never writes a guess) when the store
/// is unavailable, the prefix does not resolve, or it is ambiguous.
fn resolve_ticket_prefix(
    store: Option<&TicketStore>,
    prefix: &str,
) -> Option<String> {
    let store = store?;
    resolve_uuid_with_prefix(store, prefix)
        .ok()
        .map(|uuid| uuid.to_string())
}
