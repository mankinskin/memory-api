//! Read-only projection of a ticket's aggregated parts down to a named
//! [`ViewProfile`] or an explicit list of part kinds (spec 24b3d22b, ticket
//! 4c7b884e). Sits above [`TicketFs::read`]/[`TicketFs::load_parts`]: every
//! transport (store API, CLI, MCP, HTTP) calls [`TicketStore::project`]
//! rather than re-implementing selection so the semantics never drift per
//! transport. This module never writes.

use std::collections::{
    BTreeMap,
    BTreeSet,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::TicketStore;
use crate::{
    error::StorageError,
    model::{
        parts::{
            CORE_PART_KINDS,
            PartKindClass,
            ViewProfile,
            classify_part_kind,
        },
        ticket::{
            TicketManifestExt,
            TicketRefEntry,
        },
    },
    storage::ticket_fs::{
        LoadedPart,
        PartsLoadReport,
        TicketFs,
    },
};

/// A caller's requested read projection: a named [`ViewProfile`] or an
/// explicit list of part kinds. Mutually exclusive by construction — the
/// wire-level decoder ([`Self::decode`]) is the one place "both supplied"
/// is rejected (AC3), so no caller downstream can construct a value that
/// silently favors one over the other.
#[derive(Debug, Clone)]
pub enum ReadProjection {
    Profile(ViewProfile),
    Kinds(Vec<String>),
}

impl ReadProjection {
    /// Decode raw wire `view`/`parts` values. `parts` is a comma-separated
    /// list of part kinds. Returns `Ok(None)` when neither is supplied, so
    /// the caller can default to [`ViewProfile::Summary`] (AC4).
    pub fn decode(
        view: Option<&str>,
        parts: Option<&str>,
    ) -> Result<Option<Self>, StorageError> {
        match (view, parts) {
            (Some(_), Some(_)) => Err(StorageError::Other(
                "cannot pass both `view` and `parts`: choose a named view profile or an explicit part-kind list, not both".to_string(),
            )),
            (Some(view), None) => {
                Ok(Some(Self::Profile(ViewProfile::parse(view)?)))
            },
            (None, Some(parts)) => {
                let kinds: Vec<String> = parts
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                if kinds.is_empty() {
                    return Err(StorageError::Other(
                        "`parts` must name at least one part kind".to_string(),
                    ));
                }
                Ok(Some(Self::Kinds(kinds)))
            },
            (None, None) => Ok(None),
        }
    }
}

/// A single projected part. Carries any amendments inlined immediately
/// beneath it (oldest first, newest last) when the active profile inlines
/// amendments; empty otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectedPart {
    pub id: Uuid,
    pub kind: String,
    pub frozen: bool,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<Uuid>,
    pub content: String,
    pub implicit: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub amendments: Vec<ProjectedPart>,
}

/// A projected ticket read: selected metadata, parts (in manifest order),
/// and — for profiles that include them — typed refs.
#[derive(Debug, Clone, Serialize)]
pub struct TicketProjection {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub fields: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub parts: Vec<ProjectedPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refs: Option<Vec<TicketRefEntry>>,
}

impl TicketStore {
    /// Project a ticket's parts and metadata down to the requested `view`
    /// profile or explicit part-kind list. Read-only.
    ///
    /// A legacy ticket with no `[[parts]]` table still projects correctly:
    /// [`TicketFs::load_parts`] synthesizes its sole implicit `objective`
    /// part before selection runs, so every profile that includes
    /// `objective` (all four) sees it (AC8).
    ///
    /// A requested-but-absent core kind yields no entry for that kind, not
    /// an error (AC2). An explicit kind that is neither a core kind nor
    /// present anywhere on this ticket is rejected, naming the valid
    /// vocabulary.
    pub fn project(
        &self,
        id: &Uuid,
        projection: &ReadProjection,
    ) -> Result<TicketProjection, StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        let manifest = TicketFs::read(&indexed.path)?;
        let report = TicketFs::load_parts(&indexed.path, &manifest)?;

        let (profile, selected_kinds) = match projection {
            ReadProjection::Profile(profile) => {
                (Some(*profile), profile.kinds().map(|k| {
                    k.iter().map(|s| s.to_string()).collect::<Vec<_>>()
                }))
            },
            ReadProjection::Kinds(kinds) => {
                validate_explicit_kinds(kinds, &report)?;
                (None, Some(kinds.clone()))
            },
        };

        let inline_amendments =
            profile.map(ViewProfile::inlines_amendments).unwrap_or(false);
        let parts =
            select_parts(&report, selected_kinds.as_deref(), inline_amendments);

        let refs = profile
            .map(ViewProfile::includes_refs)
            .unwrap_or(false)
            .then(|| manifest.refs());

        Ok(TicketProjection {
            id: manifest.id,
            created_at: manifest.created_at,
            fields: manifest.extra,
            profile: profile.map(|p| p.as_str().to_string()),
            parts,
            refs,
        })
    }
}

/// Reject an explicit `parts` kind that is neither a schema-validated core
/// kind (always known, present-or-empty per AC2) nor an attachment kind
/// actually present on this ticket. Free-form kind vocabulary is per-ticket,
/// not global, so "unknown" here means "not core and not on this ticket".
fn validate_explicit_kinds(
    kinds: &[String],
    report: &PartsLoadReport,
) -> Result<(), StorageError> {
    let present: BTreeSet<&str> =
        report.parts.iter().map(|p| p.kind.as_str()).collect();
    for kind in kinds {
        match classify_part_kind(kind)? {
            PartKindClass::Core(_) => {},
            PartKindClass::Attachment(ref k) => {
                if !present.contains(k.as_str()) {
                    let mut known: Vec<&str> = CORE_PART_KINDS.to_vec();
                    known.extend(present.iter().copied());
                    return Err(StorageError::Other(format!(
                        "unknown part kind '{kind}': valid kinds for this ticket are {}",
                        known.join(", ")
                    )));
                }
            },
        }
    }
    Ok(())
}

fn select_parts(
    report: &PartsLoadReport,
    kinds: Option<&[String]>,
    inline_amendments: bool,
) -> Vec<ProjectedPart> {
    let is_selected =
        |kind: &str| kinds.map(|list| list.iter().any(|k| k == kind)).unwrap_or(true);

    let mut amendments_by_parent: BTreeMap<Uuid, Vec<&LoadedPart>> =
        BTreeMap::new();
    if inline_amendments {
        for part in &report.parts {
            if part.kind == "amendment" {
                if let Some(parent) = part.supersedes {
                    amendments_by_parent.entry(parent).or_default().push(part);
                }
            }
        }
        for children in amendments_by_parent.values_mut() {
            children.sort_by_key(|p| p.created_at);
        }
    }

    let mut consumed: BTreeSet<Uuid> = BTreeSet::new();
    let mut out = Vec::new();

    for part in &report.parts {
        if inline_amendments && part.kind == "amendment" {
            // Nested under its parent below, or surfaced as an orphan
            // afterward — never a top-level entry alongside a trailer.
            continue;
        }
        if !is_selected(&part.kind) {
            continue;
        }
        let mut projected = to_projected(part);
        if let Some(children) = amendments_by_parent.get(&part.id) {
            for child in children {
                consumed.insert(child.id);
                projected.amendments.push(to_projected(child));
            }
        }
        out.push(projected);
    }

    if inline_amendments {
        for part in &report.parts {
            if part.kind == "amendment"
                && !consumed.contains(&part.id)
                && is_selected(&part.kind)
            {
                out.push(to_projected(part));
            }
        }
    }

    out
}

fn to_projected(part: &LoadedPart) -> ProjectedPart {
    ProjectedPart {
        id: part.id,
        kind: part.kind.clone(),
        frozen: part.frozen,
        created_at: part.created_at,
        supersedes: part.supersedes,
        content: part.content.clone(),
        implicit: part.implicit,
        amendments: Vec::new(),
    }
}
