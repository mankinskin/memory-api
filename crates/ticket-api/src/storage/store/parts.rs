use serde_json::Value;
use uuid::Uuid;

use super::TicketStore;
use crate::{
    error::StorageError,
    model::ticket::TicketManifest,
    storage::ticket_fs::TicketFs,
};

/// History field key recording which part `id` a per-part write or undo
/// changed. Present only on revisions produced by
/// [`TicketStore::write_part`]/[`TicketStore::undo_part`].
pub const PART_HISTORY_ID_KEY: &str = "__previous_part_id__";
/// History field key carrying the prior content of the part named by
/// [`PART_HISTORY_ID_KEY`]; `Value::Null` when the write created the part.
pub const PART_HISTORY_CONTENT_KEY: &str = "__previous_part_content__";

impl TicketStore {
    /// The single, mandatory gate every part-write path must call before
    /// mutating a part's content (spec 24b3d22b, ticket f9e70385, AC7).
    ///
    /// Rejects with [`StorageError::FrozenPartWrite`] when `part_id`
    /// addresses an existing part currently `frozen` (i.e. a planning part
    /// on a ticket that has entered `planned`). No-ops (returns `Ok(())`)
    /// for a `part_id` with no existing manifest entry (a fresh part being
    /// created) or an existing, unfrozen part.
    ///
    /// Called by [`Self::write_part`], [`Self::undo_part`], and
    /// [`Self::enforce_description_write_gate`] (the legacy
    /// `description.md` path used by `update`/`apply_revert`) — the only
    /// content-write entry points reachable from outside this module.
    /// [`TicketFs::write_part`] and [`TicketFs::write_description`] are
    /// both `pub(crate)` with no caller outside this gate, so there is no
    /// alternate path that can touch frozen content without going through
    /// it.
    pub fn enforce_part_write_gate(
        &self,
        id: &Uuid,
        part_id: Uuid,
    ) -> Result<(), StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        let manifest = TicketFs::read(&indexed.path)?;
        if let Some(part) =
            manifest.parts().into_iter().find(|p| p.id == part_id)
        {
            if part.frozen {
                return Err(StorageError::FrozenPartWrite {
                    ticket: *id,
                    part_id,
                    kind: part.kind,
                    freezing_state: "planned".to_string(),
                });
            }
        }
        Ok(())
    }

    /// Routes the legacy `description.md` write path (`update`/`apply_revert`)
    /// through [`Self::enforce_part_write_gate`] so it is rejected exactly
    /// like a part-addressed write when frozen (spec 24b3d22b, ticket
    /// f9e70385, AC7). `description.md` backs the implicit `objective` part
    /// for legacy tickets with no `[[parts]]` table, or an explicit
    /// `objective` entry once one has been materialized (e.g. by plan
    /// freezing); either way, the same gate call is reused rather than a
    /// second parallel frozen check that could drift from it.
    pub(crate) fn enforce_description_write_gate(
        &self,
        id: &Uuid,
    ) -> Result<(), StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        let manifest = TicketFs::read(&indexed.path)?;
        let objective_part_id = manifest
            .parts()
            .into_iter()
            .find(|p| p.kind == "objective")
            .map(|p| p.id)
            .unwrap_or_else(|| {
                crate::storage::ticket_fs::implicit_objective_part_id(
                    manifest.id,
                )
            });
        self.enforce_part_write_gate(id, objective_part_id)
    }

    /// Write content to a single content part of a ticket, addressed by its
    /// stable opaque `part_id` — never by `kind` or manifest index. Creates
    /// the part (with `kind`) if `part_id` does not yet exist.
    ///
    /// Only the addressed part's file is read or written: writing a
    /// `review` or `validation` part never touches `objective` or
    /// `description.md` (AC4 of ticket 3d952036). A history revision
    /// records the prior content of only the changed part, so
    /// [`Self::undo_part`] can restore it without disturbing any other
    /// part (AC5).
    ///
    /// Rejects with [`StorageError::FrozenPartWrite`] when `part_id`
    /// addresses a part frozen by plan freezing (spec 24b3d22b, ticket
    /// f9e70385, AC2/AC3); the part file is left byte-identical.
    pub fn write_part(
        &self,
        id: &Uuid,
        part_id: Uuid,
        kind: &str,
        content: &str,
        author: Option<&str>,
    ) -> Result<TicketManifest, StorageError> {
        self.enforce_part_write_gate(id, part_id)?;
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        let (manifest, prior_content) =
            TicketFs::write_part(&indexed.path, part_id, kind, content, None)?;

        let mut history_fields = manifest.extra.clone();
        history_fields.insert(
            PART_HISTORY_ID_KEY.to_string(),
            Value::String(part_id.to_string()),
        );
        history_fields.insert(
            PART_HISTORY_CONTENT_KEY.to_string(),
            prior_content.map(Value::String).unwrap_or(Value::Null),
        );
        let _ = TicketFs::append_history(
            &indexed.path,
            history_fields,
            author.map(str::to_string),
        );

        Ok(manifest)
    }

    /// Create an `amendment` part recording a correction that supersedes a
    /// frozen planning part (spec 24b3d22b, ticket f9e70385, AC6). Sets
    /// `supersedes` to `supersedes_part_id` on the new part; `amendment` is
    /// never frozen, so this always succeeds regardless of the target
    /// part's frozen state. The amendment is retrievable alongside the
    /// part it supersedes via [`TicketFs::load_parts`]' `supersedes` field.
    pub fn write_amendment_part(
        &self,
        id: &Uuid,
        part_id: Uuid,
        content: &str,
        supersedes_part_id: Uuid,
        author: Option<&str>,
    ) -> Result<TicketManifest, StorageError> {
        self.enforce_part_write_gate(id, part_id)?;
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        let (manifest, prior_content) = TicketFs::write_part(
            &indexed.path,
            part_id,
            "amendment",
            content,
            Some(supersedes_part_id),
        )?;

        let mut history_fields = manifest.extra.clone();
        history_fields.insert(
            PART_HISTORY_ID_KEY.to_string(),
            Value::String(part_id.to_string()),
        );
        history_fields.insert(
            PART_HISTORY_CONTENT_KEY.to_string(),
            prior_content.map(Value::String).unwrap_or(Value::Null),
        );
        let _ = TicketFs::append_history(
            &indexed.path,
            history_fields,
            author.map(str::to_string),
        );

        Ok(manifest)
    }

    /// Restore a part to the content it held immediately before its most
    /// recent [`Self::write_part`]/[`Self::undo_part`] revision. Only that
    /// part's file is touched.
    ///
    /// Errors if no history revision changed this part, or if the most
    /// recent write to it was its initial creation (no prior content to
    /// restore). Rejects with [`StorageError::FrozenPartWrite`] when the
    /// part is currently frozen (AC7): undo is a write, and is not a
    /// privileged bypass of the freeze gate.
    pub fn undo_part(
        &self,
        id: &Uuid,
        part_id: Uuid,
        author: Option<&str>,
    ) -> Result<TicketManifest, StorageError> {
        self.enforce_part_write_gate(id, part_id)?;
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        let history = TicketFs::read_history(&indexed.path)?;
        let part_id_str = part_id.to_string();
        let revision = history
            .iter()
            .rev()
            .find(|rev| {
                rev.fields.get(PART_HISTORY_ID_KEY)
                    == Some(&Value::String(part_id_str.clone()))
            })
            .ok_or_else(|| {
                StorageError::Other(format!(
                    "no history revision found for part {part_id}"
                ))
            })?;

        let prior = match revision.fields.get(PART_HISTORY_CONTENT_KEY) {
            Some(Value::String(text)) => text.clone(),
            _ => {
                return Err(StorageError::Other(format!(
                    "cannot undo part {part_id}: the most recent write \
                     created it, so there is no prior content to restore"
                )));
            },
        };

        let (manifest, _) =
            TicketFs::write_part(&indexed.path, part_id, "", &prior, None)?;

        let mut history_fields = manifest.extra.clone();
        history_fields.insert(
            PART_HISTORY_ID_KEY.to_string(),
            Value::String(part_id_str),
        );
        history_fields.insert(PART_HISTORY_CONTENT_KEY.to_string(), Value::Null);
        let _ = TicketFs::append_history(
            &indexed.path,
            history_fields,
            author.map(str::to_string),
        );

        Ok(manifest)
    }
}
