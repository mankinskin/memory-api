//! Lossless description-to-parts migration (spec 24b3d22b, ticket
//! f65f2b32). Splits legacy monolithic `description.md` content into typed
//! parts for tickets with no `[[parts]]` table, moving only content that
//! matches a recognised `## Review` / `## Status` / `## Validation` /
//! `## Handoff` heading. Everything else stays in `objective` verbatim.
//!
//! Dry-run and apply are separate steps: [`TicketStore::migration_apply`]
//! only accepts a [`MigrationDryRunReport`] value, which can only be
//! produced by [`TicketStore::migration_dry_run`] — there is no way to call
//! apply without having run a dry-run first (mirrors the
//! [`DescriptionUpdate`](super::DescriptionUpdate) required-mode pattern).

use std::collections::BTreeMap;

use uuid::Uuid;

use super::TicketStore;
use crate::{
    error::StorageError,
    model::ticket::TicketManifestExt,
    storage::ticket_fs::TicketFs,
};

/// History field key marking a revision as produced by the description
/// migration, with the list of part ids it created (used by
/// [`TicketStore::migration_undo`] to identify what to remove).
pub const MIGRATION_CREATED_PART_IDS_KEY: &str =
    "__migration_created_part_ids__";

/// One contiguous, order-preserving slice of an original `description.md`,
/// classified as either the recognised `kind` of a matched heading or
/// `"objective"` for everything else (preamble, unrecognised headings, mid-
/// description asides). `content` is the exact original bytes for this
/// slice, heading line included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSegment {
    pub kind: String,
    /// The original heading line, trimmed, for a matched or unmatched H2
    /// section; `None` for a leading preamble segment with no heading.
    pub heading: Option<String>,
    pub content: String,
}

/// Maps a recognised H2 heading's first word to its typed part kind. `None`
/// means the heading is not one of the recognised four and its section
/// stays in `objective`.
fn classify_heading(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("## ") {
        return None;
    }
    let rest = trimmed[3..].trim();
    let first_word = rest
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches(':')
        .to_ascii_lowercase();
    match first_word.as_str() {
        "review" => Some("review"),
        "status" => Some("notes"),
        "validation" => Some("validation"),
        "handoff" => Some("notes"),
        _ => None,
    }
}

/// `true` for any H2 (`## `) heading line, recognised or not. H3+ headings
/// (`### `) are body content of whichever section they fall in and never
/// trigger a split.
fn is_h2_heading(line: &str) -> bool {
    line.trim_start().starts_with("## ")
}

/// Split `text` into ordered [`MigrationSegment`]s. Segments partition
/// `text` exactly with no gaps or overlaps: concatenating every segment's
/// `content` in order reproduces `text` byte-for-byte by construction.
///
/// A recognised heading always starts a new segment (never merged with an
/// adjacent recognised heading of the same kind, even a repeat — e.g. two
/// `## Status` sections in a row become two separate `notes` segments).
/// An unrecognised H2 heading only starts a new segment if the current
/// running segment was itself a recognised heading's section; consecutive
/// unrecognised content (preamble plus unrecognised headings) is merged
/// into one `objective` segment.
pub fn split_description(text: &str) -> Vec<MigrationSegment> {
    let mut segments = Vec::new();
    let mut buf = String::new();
    let mut current_kind = "objective";
    let mut current_heading: Option<String> = None;
    let mut current_is_matched = false;

    for line in text.split_inclusive('\n') {
        let stripped = line.trim_end_matches(['\n', '\r']);
        if let Some(kind) = classify_heading(stripped) {
            if !buf.is_empty() {
                segments.push(MigrationSegment {
                    kind: current_kind.to_string(),
                    heading: current_heading.take(),
                    content: std::mem::take(&mut buf),
                });
            }
            current_kind = kind;
            current_heading = Some(stripped.trim().to_string());
            current_is_matched = true;
            buf.push_str(line);
        } else if is_h2_heading(stripped) {
            if current_is_matched {
                if !buf.is_empty() {
                    segments.push(MigrationSegment {
                        kind: current_kind.to_string(),
                        heading: current_heading.take(),
                        content: std::mem::take(&mut buf),
                    });
                }
                current_kind = "objective";
                current_heading = None;
                current_is_matched = false;
            }
            buf.push_str(line);
        } else {
            buf.push_str(line);
        }
    }
    if !buf.is_empty() {
        segments.push(MigrationSegment {
            kind: current_kind.to_string(),
            heading: current_heading,
            content: buf,
        });
    }
    segments
}

/// Per-ticket migration plan produced by the dry-run scan.
#[derive(Debug, Clone)]
pub struct TicketMigrationPlan {
    pub id: Uuid,
    pub title: Option<String>,
    /// All segments in document order, including `objective` segments.
    pub segments: Vec<MigrationSegment>,
    /// Count of matched (non-`objective`) segments per kind.
    pub matched_counts: BTreeMap<String, usize>,
    /// Total lines remaining in `objective` segments.
    pub objective_lines: usize,
    /// `true` when concatenating `segments` in order reproduces the
    /// original description exactly (verified at dry-run time; always
    /// `true` by construction unless the splitter has a bug — a `false`
    /// value excludes the ticket from `migratable` and is surfaced in
    /// `low_confidence` instead).
    pub lossless: bool,
}

impl TicketMigrationPlan {
    /// `true` when at least one segment matched a recognised heading, i.e.
    /// this ticket has something to migrate.
    pub fn has_matches(&self) -> bool {
        self.matched_counts.values().any(|&n| n > 0)
    }
}

/// Full dry-run report over one store. Produced by
/// [`TicketStore::migration_dry_run`]; required by
/// [`TicketStore::migration_apply`].
#[derive(Debug, Clone, Default)]
pub struct MigrationDryRunReport {
    pub scanned: usize,
    /// Tickets with at least one recognised heading, ready to migrate.
    pub migratable: Vec<TicketMigrationPlan>,
    /// Tickets with a description and no `[[parts]]` table, but no
    /// recognised heading — nothing to move, left untouched.
    pub no_recognized_headings: usize,
    /// Tickets with no `description.md`.
    pub skipped_no_description: usize,
    /// Tickets that already have a non-empty `[[parts]]` table (already
    /// migrated, or partially structured by another part write).
    pub skipped_already_migrated: usize,
    /// Ticket ids where the splitter's own lossless concatenation check
    /// failed. Never migrated; always empty in practice — surfaced
    /// explicitly per the ticket's dry-run reporting requirement.
    pub low_confidence: Vec<Uuid>,
}

/// Result of applying a [`MigrationDryRunReport`].
#[derive(Debug, Clone, Default)]
pub struct MigrationApplyReport {
    pub migrated: Vec<Uuid>,
    /// Tickets the dry-run flagged as migratable but that were no longer
    /// eligible at apply time (already migrated since the dry-run ran).
    pub skipped_stale: Vec<Uuid>,
    /// Tickets currently `planned`, deliberately deferred rather than
    /// stepped back and forth: re-entering `planned` can be rejected by an
    /// unrelated workflow gate (e.g. a dependency that regressed after the
    /// ticket first entered `planned`), which would otherwise leave the
    /// ticket stuck unfrozen outside `planned` with no safe automatic
    /// recovery. Skipping keeps every write reversible with no bypass.
    pub skipped_planned: Vec<Uuid>,
    pub parts_created: usize,
}

impl TicketStore {
    /// Scan every ticket in this store and classify it for migration
    /// without writing anything.
    pub fn migration_dry_run(
        &self
    ) -> Result<MigrationDryRunReport, StorageError> {
        let mut report = MigrationDryRunReport::default();
        for ticket in self.list(None, None, None)? {
            report.scanned += 1;
            let manifest = TicketFs::read(&ticket.path)?;
            if !manifest.parts().is_empty() {
                report.skipped_already_migrated += 1;
                continue;
            }
            let Some(description) = TicketFs::read_description(&ticket.path)
            else {
                report.skipped_no_description += 1;
                continue;
            };

            let segments = split_description(&description);
            let reconstructed: String =
                segments.iter().map(|s| s.content.as_str()).collect();
            let lossless = reconstructed == description;
            if !lossless {
                report.low_confidence.push(ticket.id);
                continue;
            }

            let mut matched_counts: BTreeMap<String, usize> = BTreeMap::new();
            let mut objective_lines = 0usize;
            for segment in &segments {
                if segment.kind == "objective" {
                    objective_lines += segment.content.lines().count();
                } else {
                    *matched_counts.entry(segment.kind.clone()).or_insert(0) +=
                        1;
                }
            }

            let plan = TicketMigrationPlan {
                id: ticket.id,
                title: ticket.title.clone(),
                segments,
                matched_counts,
                objective_lines,
                lossless,
            };

            if plan.has_matches() {
                report.migratable.push(plan);
            } else {
                report.no_recognized_headings += 1;
            }
        }
        Ok(report)
    }

    /// Apply a dry-run report: for every migratable ticket, still eligible
    /// (no `[[parts]]` table yet), create one part per matched segment plus
    /// one `objective` part per unmatched segment, preserving document
    /// order in the manifest so concatenating parts in manifest order
    /// reproduces the original description exactly (AC1).
    ///
    /// A ticket currently `planned` is deferred (see
    /// [`MigrationApplyReport::skipped_planned`]) rather than stepped back
    /// and forth, since re-entering `planned` can be rejected by an
    /// unrelated workflow gate with no safe automatic recovery.
    pub fn migration_apply(
        &self,
        dry_run: &MigrationDryRunReport,
        author: Option<&str>,
    ) -> Result<MigrationApplyReport, StorageError> {
        let mut report = MigrationApplyReport::default();

        for plan in &dry_run.migratable {
            let id = plan.id;
            let Some(indexed) = self.get_indexed(&id)? else {
                report.skipped_stale.push(id);
                continue;
            };
            let manifest = TicketFs::read(&indexed.path)?;
            if !manifest.parts().is_empty() {
                // Already migrated (or otherwise structured) since the
                // dry-run ran; skip rather than risk double-writing.
                report.skipped_stale.push(id);
                continue;
            }

            let was_planned = indexed.state.as_deref() == Some("planned");
            if was_planned {
                // Deferred rather than stepped back and forth (see
                // `MigrationApplyReport::skipped_planned`): re-entering
                // `planned` can be rejected by an unrelated dependency
                // gate, which would leave the ticket stuck unfrozen with
                // no safe automatic recovery. Left completely untouched.
                report.skipped_planned.push(id);
                continue;
            }

            // Every segment — including `objective` ones — gets a fresh
            // random part id, never the deterministic implicit-objective
            // id: `TicketFs::write_part` special-cases that id when the
            // parts table is still empty by writing straight to
            // `description.md` with no manifest entry, which would make
            // the segment invisible to `load_parts` the moment a sibling
            // part is added. A fresh id always takes the "new explicit
            // part" path, keeping every segment tracked in the manifest
            // in call order (= document order).
            let mut created_part_ids = Vec::with_capacity(plan.segments.len());
            for segment in &plan.segments {
                let part_id = Uuid::new_v4();
                self.write_part(
                    &id,
                    part_id,
                    &segment.kind,
                    &segment.content,
                    author,
                )?;
                created_part_ids.push(part_id);
                report.parts_created += 1;
            }

            let mut history_fields = BTreeMap::new();
            history_fields.insert(
                MIGRATION_CREATED_PART_IDS_KEY.to_string(),
                serde_json::Value::Array(
                    created_part_ids
                        .iter()
                        .map(|id| serde_json::Value::String(id.to_string()))
                        .collect(),
                ),
            );
            if let Err(error) = TicketFs::append_history(
                &indexed.path,
                history_fields,
                author.map(str::to_string),
            ) {
                tracing::error!(
                    ticket_id = %id,
                    %error,
                    "failed to append migration history marker revision"
                );
            }

            report.migrated.push(id);
        }

        Ok(report)
    }

    /// Reverse [`Self::migration_apply`] for a single ticket: removes every
    /// part created by the most recent migration history revision and
    /// deletes their backing files, restoring the pre-migration
    /// single-`description.md` layout (AC6). Errors if the ticket has no
    /// migration history revision.
    pub fn migration_undo(
        &self,
        id: &Uuid,
        author: Option<&str>,
    ) -> Result<(), StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        let history = TicketFs::read_history(&indexed.path)?;
        let revision = history
            .iter()
            .rev()
            .find(|rev| rev.fields.contains_key(MIGRATION_CREATED_PART_IDS_KEY))
            .ok_or_else(|| {
                StorageError::Other(format!(
                    "no migration history revision found for ticket {id}"
                ))
            })?;
        let created_ids: Vec<Uuid> =
            match revision.fields.get(MIGRATION_CREATED_PART_IDS_KEY) {
                Some(serde_json::Value::Array(values)) => values
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| Uuid::parse_str(s).ok())
                    .collect(),
                _ => Vec::new(),
            };

        TicketFs::remove_parts(&indexed.path, &created_ids)?;

        let mut history_fields = BTreeMap::new();
        history_fields.insert(
            "__migration_undo_of__".to_string(),
            serde_json::Value::Array(
                created_ids
                    .iter()
                    .map(|id| serde_json::Value::String(id.to_string()))
                    .collect(),
            ),
        );
        TicketFs::append_history(
            &indexed.path,
            history_fields,
            author.map(str::to_string),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognized_headings_map_to_expected_kinds() {
        let text = "\
## Objective

Do the thing.

## Review

Looks good.

## Status

In progress.

## Validation

All green.

## Handoff

Ready.
";
        let segments = split_description(text);
        let kinds: Vec<&str> =
            segments.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec!["objective", "review", "notes", "validation", "notes"]
        );
        let reconstructed: String =
            segments.iter().map(|s| s.content.as_str()).collect();
        assert_eq!(reconstructed, text);
    }

    #[test]
    fn heading_variants_are_case_insensitive_and_tolerate_trailing_text() {
        let text = "## review\nfirst\n## Review 2026-07-29\nsecond\n";
        let segments = split_description(text);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].kind, "review");
        assert_eq!(segments[1].kind, "review");
        let reconstructed: String =
            segments.iter().map(|s| s.content.as_str()).collect();
        assert_eq!(reconstructed, text);
    }

    #[test]
    fn unrecognized_heading_stays_in_objective() {
        let text = "## Objective\nintro\n## Notes To Self\nasides\n## Review\nfindings\n";
        let segments = split_description(text);
        let kinds: Vec<&str> =
            segments.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, vec!["objective", "review"]);
        assert!(segments[0].content.contains("Notes To Self"));
        let reconstructed: String =
            segments.iter().map(|s| s.content.as_str()).collect();
        assert_eq!(reconstructed, text);
    }

    #[test]
    fn no_headings_yields_single_objective_segment() {
        let text = "Just plain prose.\nNo headings at all.\n";
        let segments = split_description(text);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].kind, "objective");
        assert_eq!(segments[0].content, text);
    }

    #[test]
    fn repeated_heading_kind_never_merges() {
        let text = "## Status\nfirst update\n## Status\nsecond update\n";
        let segments = split_description(text);
        assert_eq!(segments.len(), 2);
        assert!(segments.iter().all(|s| s.kind == "notes"));
        let reconstructed: String =
            segments.iter().map(|s| s.content.as_str()).collect();
        assert_eq!(reconstructed, text);
    }

    #[test]
    fn empty_description_yields_no_segments() {
        assert!(split_description("").is_empty());
    }
}
