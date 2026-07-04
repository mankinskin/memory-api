use std::{
    path::{
        Path,
        PathBuf,
    },
    thread,
    time::Duration,
};

use tantivy::{
    Index,
    IndexWriter,
    TantivyDocument,
    TantivyError,
    Term,
    schema::{
        FAST,
        Field,
        INDEXED,
        STORED,
        STRING,
        Schema,
        TEXT,
        Value as TantivyValue,
    },
};
use uuid::Uuid;

use crate::{
    error::StorageError,
    model::query::{
        Expr,
        ValueExpr,
    },
};

#[derive(Debug)]
pub struct SearchResult {
    pub id: Uuid,
    pub title: Option<String>,
    pub state: Option<String>,
    /// Kept as `ticket_type` for backward compatibility with downstream consumers.
    pub ticket_type: Option<String>,
    pub snippet: Option<String>,
    pub score: f32,
}

pub struct SearchFields {
    pub id: Field,
    pub title: Field,
    pub body: Field,
    pub state: Field,
    pub ticket_type: Field,
    pub created_at: Field,
    pub effort: Field,
}

#[derive(Debug, Clone)]
pub struct SearchDocumentInput {
    pub id: Uuid,
    pub title: Option<String>,
    pub body: Option<String>,
    pub state: Option<String>,
    pub ticket_type: Option<String>,
    pub created_at: Option<String>,
    pub effort: Option<String>,
}

/// Tantivy-backed full-text search index.
///
/// # Windows file-sharing note
///
/// Tantivy's default `MmapDirectory` opens segment files with `FILE_SHARE_READ`
/// only.  On Windows this prevents any other process from writing to (or GC-
/// deleting) those segment files while the mapping is alive.  To avoid blocking
/// concurrent CLI writers when a long-running viewer server is running, this
/// struct stores only the **directory path** and opens (and immediately drops)
/// a fresh `Index` for every operation.  Between operations no OS file handles
/// are held, so the CLI can write freely.
pub struct TantivySearchIndex {
    dir: PathBuf,
    fields: SearchFields,
}

/// A single readiness invariant the search index must satisfy before it can
/// serve correct results for **any** read operation.
///
/// The full set of conditions that must hold before a read can be trusted:
///
/// 1. [`IndexInvariant::DirectoryExists`] — the index directory is present on
///    disk so Tantivy has somewhere to read segments from.
/// 2. [`IndexInvariant::Openable`] — a non-empty directory opens as a
///    structurally valid Tantivy index (its `meta.json` is present and parses,
///    and segment files are not truncated or corrupt).
/// 3. [`IndexInvariant::SchemaCurrent`] — the on-disk schema matches the
///    current [`build_schema`] layout exactly: field set, declaration order,
///    value types, and indexing options (including `FAST`). A stale layout
///    makes Tantivy's fast-field writer index past its `fast_field_names`
///    vector and panic on a background thread.
///
/// These invariants are enforced *proactively* on every interaction with the
/// index — both reads and writes — by [`TantivySearchIndex::ensure_ready`],
/// which validates each condition and heals any violation before the operation
/// proceeds, rather than catching errors after the fact.
///
/// An **empty** directory is always valid: it is not yet an index and is
/// created from the current schema on first use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexInvariant {
    /// The index directory exists on disk.
    DirectoryExists,
    /// A non-empty directory opens as a valid Tantivy index (not corrupt,
    /// truncated, or missing `meta.json`).
    Openable,
    /// The on-disk schema matches the current [`build_schema`] exactly.
    SchemaCurrent,
}

const SEARCH_IO_RETRY_ATTEMPTS: usize = 8;
const SEARCH_IO_RETRY_BASE_DELAY_MS: u64 = 25;

impl TantivySearchIndex {
    pub fn open_or_create(dir: &Path) -> Result<Self, StorageError> {
        let (_, fields) = build_schema();
        let index = Self {
            dir: dir.to_path_buf(),
            fields,
        };

        // Validate and heal every readiness invariant up front, then drop the
        // returned handle so no Windows MmapDirectory file handles are held
        // between operations.
        index.ensure_ready()?;

        Ok(index)
    }

    /// Open a fresh, **ready** `Index` handle for a single operation, then drop
    /// it.
    ///
    /// Every read and write goes through here, so this is the single choke
    /// point where [`Self::ensure_ready`] enforces the index invariants before
    /// the caller touches the index.
    fn open_index(&self) -> Result<Index, StorageError> {
        self.ensure_ready()
    }

    fn make_writer(index: &Index) -> Result<IndexWriter, StorageError> {
        index
            .writer(50_000_000)
            .map_err(|e| StorageError::SearchIndex(e.to_string()))
    }

    fn add_document(
        &self,
        writer: &mut IndexWriter,
        doc: &SearchDocumentInput,
    ) -> Result<(), StorageError> {
        writer.delete_term(Term::from_field_text(
            self.fields.id,
            &doc.id.to_string(),
        ));

        let mut tantivy_doc = TantivyDocument::default();
        tantivy_doc.add_text(self.fields.id, doc.id.to_string());
        if let Some(title) = &doc.title {
            tantivy_doc.add_text(self.fields.title, title);
        }
        if let Some(body) = &doc.body {
            tantivy_doc.add_text(self.fields.body, body);
        }
        tantivy_doc.add_text(
            self.fields.state,
            doc.state.as_deref().unwrap_or(""),
        );
        tantivy_doc.add_text(
            self.fields.ticket_type,
            doc.ticket_type.as_deref().unwrap_or(""),
        );
        tantivy_doc.add_text(
            self.fields.created_at,
            doc.created_at.as_deref().unwrap_or(""),
        );
        let effort_value = doc
            .effort
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        tantivy_doc.add_i64(self.fields.effort, effort_value);
        writer.add_document(tantivy_doc).map_err(|e: TantivyError| {
            StorageError::SearchIndex(e.to_string())
        })?;
        Ok(())
    }

    fn is_retryable_search_error(error: &StorageError) -> bool {
        let StorageError::SearchIndex(message) = error else {
            return false;
        };

        let lower = message.to_ascii_lowercase();
        lower.contains("permissiondenied")
            || lower.contains("os error 5")
            || lower.contains("access is denied")
            || lower.contains("zugriff verweigert")
    }

    fn with_retry<T, F>(mut op: F) -> Result<T, StorageError>
    where
        F: FnMut() -> Result<T, StorageError>,
    {
        let mut delay_ms = SEARCH_IO_RETRY_BASE_DELAY_MS;
        for attempt in 0..SEARCH_IO_RETRY_ATTEMPTS {
            match op() {
                Ok(value) => return Ok(value),
                Err(error)
                    if attempt + 1 < SEARCH_IO_RETRY_ATTEMPTS
                        && Self::is_retryable_search_error(&error) =>
                {
                    thread::sleep(Duration::from_millis(delay_ms));
                    delay_ms = (delay_ms * 2).min(500);
                },
                Err(error) => return Err(error),
            }
        }

        unreachable!("retry loop must return or error")
    }

    /// Run a read operation, converting a Tantivy panic into a recoverable
    /// error.
    ///
    /// Tantivy panics (rather than returning an error) on some classes of
    /// on-disk corruption — for example a truncated segment whose slice offset
    /// underflows (`attempt to subtract with overflow`). On the calling thread
    /// such a panic would abort the read, so it is caught here and mapped to a
    /// `SearchIndex` error containing `"panic"`, which
    /// [`Self::should_rebuild_search_index`] and
    /// [`Self::is_rebuildable_read_failure`] both classify as rebuild-worthy.
    /// Catching is safe because the search index is a derived cache that is
    /// rebuilt from the filesystem source of truth on the next operation.
    fn catch_index_panic<T, F>(op: F) -> Result<T, StorageError>
    where
        F: FnOnce() -> Result<T, StorageError>,
    {
        // Suppress the default panic hook's stderr backtrace for this scoped
        // read: a caught, recovered-from corruption panic is not a crash.
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(op));
        std::panic::set_hook(previous_hook);

        match result {
            Ok(inner) => inner,
            Err(_) => Err(StorageError::SearchIndex(
                "search index read panicked (corrupt on-disk segment); \
                 rebuild required"
                    .to_string(),
            )),
        }
    }


    pub fn should_rebuild_search_index(error: &StorageError) -> bool {
        let StorageError::SearchIndex(message) = error else {
            return false;
        };

        let lower = message.to_ascii_lowercase();
        lower.contains("index out of bounds")
            || lower.contains("an error occurred in a thread")
            || lower.contains("corrupt")
            || lower.contains("corrupted")
            || lower.contains("panic")
    }

    /// Whether a failure observed while **reading** the index warrants
    /// rebuilding it from the source of truth.
    ///
    /// Segment-content damage (truncated, corrupt, or missing segment files)
    /// keeps `meta.json` valid, so it passes the cheap structural and
    /// completeness checks and only surfaces when the searcher reads a segment.
    /// The error messages vary widely (`File corrupted`, `FileDoesNotExist`,
    /// `UnexpectedEof`, background-thread panics, …), so any search-index error
    /// that is **not** a transient, retryable IO/permission error is treated as
    /// on-disk damage that a rebuild repairs. Rebuilding is always safe because
    /// the filesystem entities are the authoritative source.
    pub fn is_rebuildable_read_failure(error: &StorageError) -> bool {
        matches!(error, StorageError::SearchIndex(_))
            && !Self::is_retryable_search_error(error)
    }

    pub fn reset_dir(&self) -> Result<(), StorageError> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        std::fs::create_dir_all(&self.dir)?;
        Ok(())
    }

    /// Validate and heal every [`IndexInvariant`], returning an open [`Index`]
    /// that is guaranteed to satisfy all readiness conditions.
    ///
    /// This is the single proactive gate that every read and write passes
    /// through. It enforces, in order:
    ///
    /// 1. **Directory exists** — created if missing.
    /// 2. **Openable** — a non-empty directory must open as a valid Tantivy
    ///    index; a corrupt/truncated index is rebuilt from the current schema.
    /// 3. **Schema current** — the on-disk schema must match [`build_schema`]
    ///    exactly; a stale layout is rebuilt *before* any document is written,
    ///    so the fast-field writer can never index past its `fast_field_names`
    ///    vector.
    ///
    /// Healing is proactive: schema drift and corruption are repaired here, not
    /// caught downstream after a panic or error. An empty directory is valid
    /// and is created from the current schema.
    fn ensure_ready(&self) -> Result<Index, StorageError> {
        // Invariant 1: the directory must exist.
        std::fs::create_dir_all(&self.dir)?;

        let is_empty = self
            .dir
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true);
        if is_empty {
            return self.create_fresh_index();
        }

        // Invariant 2: a non-empty directory must open as a valid index.
        let index = match Index::open_in_dir(&self.dir) {
            Ok(index) => index,
            Err(error) => {
                tracing::warn!(
                    dir = %self.dir.display(),
                    %error,
                    "search index unreadable; rebuilding from current schema",
                );
                return self.create_fresh_index();
            },
        };

        // Invariant 3: the on-disk schema must match the current schema.
        let (expected, _) = build_schema();
        if schemas_match(&index.schema(), &expected) {
            return Ok(index);
        }

        drop(index);
        tracing::warn!(
            dir = %self.dir.display(),
            "search index schema is stale; rebuilding from current schema",
        );
        self.create_fresh_index()
    }

    /// Reset the directory and create a brand-new index from the current
    /// schema. Used by [`Self::ensure_ready`] to heal a missing, corrupt, or
    /// stale-schema index.
    fn create_fresh_index(&self) -> Result<Index, StorageError> {
        self.reset_dir()?;
        let (schema, _) = build_schema();
        Index::create_in_dir(&self.dir, schema)
            .map_err(|e| StorageError::SearchIndex(e.to_string()))
    }

    /// Validate every [`IndexInvariant`] **without** mutating the index and
    /// return the first violation, if any.
    ///
    /// This is the read-only counterpart to [`Self::ensure_ready`], intended
    /// for health checks and tests. `Ok(None)` means the index is ready (or is
    /// an empty directory that will be created on first use).
    pub fn check_invariants(
        &self
    ) -> Result<Option<IndexInvariant>, StorageError> {
        if !self.dir.exists() {
            return Ok(Some(IndexInvariant::DirectoryExists));
        }

        let is_empty = self
            .dir
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true);
        if is_empty {
            return Ok(None);
        }

        let index = match Index::open_in_dir(&self.dir) {
            Ok(index) => index,
            Err(_) => return Ok(Some(IndexInvariant::Openable)),
        };

        let (expected, _) = build_schema();
        if !schemas_match(&index.schema(), &expected) {
            return Ok(Some(IndexInvariant::SchemaCurrent));
        }

        Ok(None)
    }

    /// Proactively validate and heal all index invariants.
    ///
    /// Thin wrapper over [`Self::ensure_ready`] for callers that want to force
    /// the index into a known-good state without performing an operation (for
    /// example before a bulk reindex). The healed index handle is dropped
    /// immediately so no file handles are retained.
    pub fn ensure_schema_current(&self) -> Result<(), StorageError> {
        self.ensure_ready().map(|_| ())
    }

    /// Proactively validate the **structural** invariants and rebuild the index
    /// when any is violated (missing directory, unopenable/corrupt index, or
    /// stale schema).
    ///
    /// Returns `true` when a rebuild occurred. Because a rebuild produces an
    /// empty index, the caller is then responsible for restoring the
    /// **completeness** invariant — re-indexing every entity from the
    /// filesystem source of truth (the search index cannot do this itself; only
    /// the owning store knows the entities). Returns `false` when the index was
    /// already structurally valid, so no repopulation is needed.
    pub fn heal_if_needed(&self) -> Result<bool, StorageError> {
        std::fs::create_dir_all(&self.dir)?;
        match self.check_invariants()? {
            None => Ok(false),
            Some(_) => {
                self.create_fresh_index()?;
                Ok(true)
            },
        }
    }

    /// Number of live documents in the index.
    ///
    /// Routes through [`Self::open_index`], so the structural invariants are
    /// enforced first; a freshly-rebuilt (healed) index reports `0`. The owning
    /// store compares this against its metadata-index entity count to detect an
    /// empty or partial search index that must be repopulated (the
    /// **completeness** invariant).
    pub fn num_docs(&self) -> Result<u64, StorageError> {
        Self::catch_index_panic(|| {
            let index = self.open_index()?;
            let reader = index
                .reader()
                .map_err(|e| StorageError::SearchIndex(e.to_string()))?;
            Ok(reader.searcher().num_docs())
        })
    }

    /// Index or update an entity document. Deletes any existing document for the
    /// same `id` first to ensure upsert semantics.
    pub fn upsert(
        &self,
        id: &Uuid,
        title: Option<&str>,
        body: Option<&str>,
        state: Option<&str>,
        entity_type: Option<&str>,
        created_at: Option<&str>,
        effort: Option<&str>,
    ) -> Result<(), StorageError> {
        self.upsert_batch(&[SearchDocumentInput {
            id: *id,
            title: title.map(str::to_string),
            body: body.map(str::to_string),
            state: state.map(str::to_string),
            ticket_type: entity_type.map(str::to_string),
            created_at: created_at.map(str::to_string),
            effort: effort.map(str::to_string),
        }])
    }

    pub fn upsert_batch(
        &self,
        docs: &[SearchDocumentInput],
    ) -> Result<(), StorageError> {
        if docs.is_empty() {
            return Ok(());
        }

        Self::with_retry(|| {
            let index = self.open_index()?;
            let mut writer = Self::make_writer(&index)?;
            for doc in docs {
                self.add_document(&mut writer, doc)?;
            }
            writer
                .commit()
                .map_err(|e| StorageError::SearchIndex(e.to_string()))?;
            // Wait for background merge threads to release all file handles before
            // returning. On Windows, MmapDirectory only allows FILE_SHARE_READ, so
            // any handle a merge thread holds will cause PermissionDenied if the
            // next writer tries to write or GC the same segment file.
            writer
                .wait_merging_threads()
                .map_err(|e| StorageError::SearchIndex(e.to_string()))?;
            drop(index);
            Ok(())
        })
    }

    pub fn remove(
        &self,
        id: &Uuid,
    ) -> Result<(), StorageError> {
        self.remove_batch(&[*id])
    }

    pub fn remove_batch(
        &self,
        ids: &[Uuid],
    ) -> Result<(), StorageError> {
        if ids.is_empty() {
            return Ok(());
        }

        Self::with_retry(|| {
            let index = self.open_index()?;
            let mut writer = Self::make_writer(&index)?;
            for id in ids {
                writer.delete_term(Term::from_field_text(
                    self.fields.id,
                    &id.to_string(),
                ));
            }
            writer.commit().map_err(|e: TantivyError| {
                StorageError::SearchIndex(e.to_string())
            })?;
            writer
                .wait_merging_threads()
                .map_err(|e| StorageError::SearchIndex(e.to_string()))?;
            drop(index);
            Ok(())
        })
    }

    /// Delete every document from the Tantivy index.
    pub fn clear_all(&self) -> Result<(), StorageError> {
        Self::with_retry(|| {
            let index = self.open_index()?;
            let mut writer = Self::make_writer(&index)?;
            writer.delete_all_documents().map_err(|e: TantivyError| {
                StorageError::SearchIndex(e.to_string())
            })?;
            writer.commit().map_err(|e: TantivyError| {
                StorageError::SearchIndex(e.to_string())
            })?;
            writer
                .wait_merging_threads()
                .map_err(|e| StorageError::SearchIndex(e.to_string()))?;
            drop(index);
            Ok(())
        })
    }

    /// Search using a parsed `Expr` AST.
    /// Returns up to `limit` results ordered by relevance score.
    pub fn search(
        &self,
        expr: &Expr,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        use tantivy::{
            collector::TopDocs,
            query::{
                AllQuery,
                BooleanQuery,
                Occur,
                Query,
                TermQuery,
            },
        };

        // Catch panics from corrupt on-disk segments so the read can be retried
        // against a rebuilt index instead of aborting the process.
        Self::catch_index_panic(|| {
        let index = self.open_index()?;
        let reader = index
            .reader()
            .map_err(|e| StorageError::SearchIndex(e.to_string()))?;
        let searcher = reader.searcher();

        let query: Box<dyn Query> = expr_to_query(expr, &self.fields, &index);

        let top_docs = searcher
            .search(&*query, &TopDocs::with_limit(limit))
            .map_err(|e| StorageError::SearchIndex(e.to_string()))?;

        let schema = index.schema();
        let mut results = Vec::with_capacity(top_docs.len());

        for (score, doc_addr) in top_docs {
            let doc = searcher
                .doc::<TantivyDocument>(doc_addr)
                .map_err(|e| StorageError::SearchIndex(e.to_string()))?;

            let id_str = get_text(&doc, self.fields.id, &schema);
            let id: Uuid = match id_str.as_deref().and_then(|s| s.parse().ok())
            {
                Some(u) => u,
                None => continue,
            };

            results.push(SearchResult {
                id,
                title: get_text(&doc, self.fields.title, &schema),
                state: get_text(&doc, self.fields.state, &schema),
                ticket_type: get_text(&doc, self.fields.ticket_type, &schema),
                snippet: get_text(&doc, self.fields.body, &schema)
                    .map(|b| truncate_snippet(&b, 120)),
                score,
            });
        }

        // Suppress unused import warnings — these are used inside expr_to_query.
        let _ = (
            AllQuery,
            BooleanQuery::new(vec![]),
            Occur::Must,
            TermQuery::new(
                Term::from_field_text(self.fields.id, ""),
                Default::default(),
            ),
        );

        Ok(results)
        })
    }
}

/// Compare two schemas structurally (field names, types, and options).
///
/// Serializing the schema captures field order, names, value types, and
/// per-field options (`FAST`, `STORED`, `INDEXED`, …), so any layout change —
/// including a newly added fast field — produces a mismatch. A serialization
/// failure conservatively reports a mismatch so the caller rebuilds.
fn schemas_match(
    a: &Schema,
    b: &Schema,
) -> bool {
    match (serde_json::to_string(a), serde_json::to_string(b)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn build_schema() -> (Schema, SearchFields) {
    let mut builder = Schema::builder();
    let id = builder.add_text_field("id", STRING | STORED);
    let title = builder.add_text_field("title", TEXT | STORED);
    let body = builder.add_text_field("body", TEXT | STORED);
    let state = builder.add_text_field("state", STRING | STORED | FAST);
    let ticket_type =
        builder.add_text_field("ticket_type", STRING | STORED | FAST);
    let created_at =
        builder.add_text_field("created_at", STRING | STORED | FAST);
    let effort =
        builder.add_i64_field("effort", INDEXED | STORED | FAST);
    let schema = builder.build();
    (
        schema,
        SearchFields {
            id,
            title,
            body,
            state,
            ticket_type,
            created_at,
            effort,
        },
    )
}

fn get_text(
    doc: &TantivyDocument,
    field: Field,
    _schema: &Schema,
) -> Option<String> {
    doc.get_first(field)
        .and_then(|v| TantivyValue::as_str(&v))
        .map(str::to_string)
}

fn expr_to_query(
    expr: &Expr,
    fields: &SearchFields,
    index: &Index,
) -> Box<dyn tantivy::query::Query> {
    match expr {
        Expr::Fts(text) => full_text_query(text, fields, index),
        Expr::Field { key, value } => field_expr_to_query(key, value, fields),
        Expr::Compare { key, op, value } =>
            compare_expr_to_query(key, *op, value, fields),
        Expr::And(exprs) => and_expr_to_query(exprs, fields, index),
        Expr::Or(exprs) => or_expr_to_query(exprs, fields, index),
        Expr::Not(expr) => not_expr_to_query(expr, fields, index),
    }
}

/// Translate a comparison predicate into a Tantivy query.
///
/// The shared parser now emits comparison operators and deep-field paths, but
/// full numeric/temporal range evaluation against fast fields is a follow-on
/// slice. For now:
/// - `Contains` reuses the substring regex query on the resolved field.
/// - `Exists` matches any document that has the resolved indexed field.
/// - Ordering comparisons (`Gt`/`Gte`/`Lt`/`Lte`) and unresolved deep fields
///   degrade to `AllQuery` so they never silently drop candidates; precise
///   evaluation is layered in when fast fields are added.
fn tantivy_field_name_for_key(key: &str) -> Option<&str> {
    match key {
        "state" | "status" => Some("state"),
        "type" | "ticket_type" => Some("ticket_type"),
        "id" => Some("id"),
        "title" => Some("title"),
        "created" | "created_at" => Some("created_at"),
        "effort" => Some("effort"),
        _ => None,
    }
}

fn compare_expr_to_query(
    key: &str,
    op: crate::model::query::CompareOp,
    value: &ValueExpr,
    fields: &SearchFields,
) -> Box<dyn tantivy::query::Query> {
    use crate::model::query::CompareOp;
    use tantivy::query::AllQuery;

    let Some(field) = search_field_for_key(key, fields) else {
        return Box::new(AllQuery);
    };
    let Some(field_name) = tantivy_field_name_for_key(key) else {
        return Box::new(AllQuery);
    };

    match (op, value) {
        (CompareOp::Contains, ValueExpr::Text(text)) =>
            substring_query_for_fields(text, &[field])
                .unwrap_or_else(|| Box::new(AllQuery)),
        (CompareOp::Exists, _) => exists_query(field),
        (CompareOp::Gt, _) | (CompareOp::Gte, _) | (CompareOp::Lt, _) | (CompareOp::Lte, _) | (CompareOp::Range, _) => {
            build_range_query(field, field_name, fields, op, value)
        }
        _ => Box::new(AllQuery),
    }
}

fn build_range_query(
    field: Field,
    field_name: &str,
    fields: &SearchFields,
    op: crate::model::query::CompareOp,
    value: &ValueExpr,
) -> Box<dyn tantivy::query::Query> {
    use crate::model::query::CompareOp;
    use tantivy::query::{RangeQuery, EmptyQuery};
    use std::ops::Bound;

    let get_term = |val_str: &str| -> Option<Term> {
        if field == fields.effort {
            val_str.parse::<i64>().ok().map(|val| Term::from_field_i64(field, val))
        } else {
            Some(Term::from_field_text(field, val_str))
        }
    };

    let field_type = if field == fields.effort {
        tantivy::schema::Type::I64
    } else {
        tantivy::schema::Type::Str
    };

    match (op, value) {
        (CompareOp::Gt, ValueExpr::Text(text)) => {
            let Some(term) = get_term(text) else {
                return Box::new(EmptyQuery);
            };
            Box::new(RangeQuery::new_term_bounds(
                field_name.to_string(),
                field_type,
                &Bound::Excluded(term),
                &Bound::Unbounded,
            ))
        }
        (CompareOp::Gte, ValueExpr::Text(text)) => {
            let Some(term) = get_term(text) else {
                return Box::new(EmptyQuery);
            };
            Box::new(RangeQuery::new_term_bounds(
                field_name.to_string(),
                field_type,
                &Bound::Included(term),
                &Bound::Unbounded,
            ))
        }
        (CompareOp::Lt, ValueExpr::Text(text)) => {
            let Some(term) = get_term(text) else {
                return Box::new(EmptyQuery);
            };
            Box::new(RangeQuery::new_term_bounds(
                field_name.to_string(),
                field_type,
                &Bound::Unbounded,
                &Bound::Excluded(term),
            ))
        }
        (CompareOp::Lte, ValueExpr::Text(text)) => {
            let Some(term) = get_term(text) else {
                return Box::new(EmptyQuery);
            };
            Box::new(RangeQuery::new_term_bounds(
                field_name.to_string(),
                field_type,
                &Bound::Unbounded,
                &Bound::Included(term),
            ))
        }
        (CompareOp::Range, ValueExpr::Range { start, end }) => {
            let Some(start_term) = get_term(start) else {
                return Box::new(EmptyQuery);
            };
            let Some(end_term) = get_term(end) else {
                return Box::new(EmptyQuery);
            };
            Box::new(RangeQuery::new_term_bounds(
                field_name.to_string(),
                field_type,
                &Bound::Included(start_term),
                &Bound::Included(end_term),
            ))
        }
        _ => Box::new(EmptyQuery),
    }
}

/// Match any document that has a non-null value in `field`.
fn exists_query(field: Field) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::{
        BooleanQuery,
        Occur,
        RegexQuery,
    };

    // A field exists when it matches any non-empty value. `.+` over the
    // indexed text approximates presence for STRING/TEXT fields.
    match RegexQuery::from_pattern(".+", field) {
        Ok(query) => Box::new(BooleanQuery::new(vec![(
            Occur::Must,
            Box::new(query) as Box<dyn tantivy::query::Query>,
        )])),
        Err(_) => Box::new(tantivy::query::AllQuery),
    }
}

fn full_text_query(
    text: &str,
    fields: &SearchFields,
    index: &Index,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::{
        AllQuery,
        BooleanQuery,
        Occur,
    };

    let mut query_parser = tantivy::query::QueryParser::for_index(
        index,
        vec![fields.title, fields.body],
    );
    query_parser.set_conjunction_by_default();
    let exact_query = query_parser.parse_query(text).ok();
    let substring_query = substring_query_for_fields(
        text,
        &[fields.title, fields.body, fields.id],
    );

    match (exact_query, substring_query) {
        (Some(exact_query), Some(substring_query)) =>
            Box::new(BooleanQuery::new(vec![
                (Occur::Should, exact_query),
                (Occur::Should, substring_query),
            ])),
        (Some(exact_query), None) => exact_query,
        (None, Some(substring_query)) => substring_query,
        (None, None) => Box::new(AllQuery),
    }
}

fn substring_query_for_fields(
    text: &str,
    fields: &[Field],
) -> Option<Box<dyn tantivy::query::Query>> {
    use tantivy::query::{
        BooleanQuery,
        Occur,
        RegexQuery,
    };

    let needle = text.trim().to_ascii_lowercase();
    if needle.is_empty() || needle.chars().any(char::is_whitespace) {
        return None;
    }

    let pattern = format!(".*{}.*", regex::escape(&needle));

    let clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = fields
        .iter()
        .copied()
        .filter_map(|field| {
            RegexQuery::from_pattern(&pattern, field).ok().map(|query| {
                (
                    Occur::Should,
                    Box::new(query) as Box<dyn tantivy::query::Query>,
                )
            })
        })
        .collect();

    if clauses.is_empty() {
        None
    } else {
        Some(Box::new(BooleanQuery::new(clauses)))
    }
}

fn id_field_query(
    text: &str,
    fields: &SearchFields,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::{
        BooleanQuery,
        Occur,
    };

    let exact_query = term_query(fields.id, text);
    match substring_query_for_fields(text, &[fields.id]) {
        Some(substring_query) => Box::new(BooleanQuery::new(vec![
            (Occur::Should, exact_query),
            (Occur::Should, substring_query),
        ])),
        None => exact_query,
    }
}

fn title_field_query(
    text: &str,
    fields: &SearchFields,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::{
        BooleanQuery,
        Occur,
    };

    let exact_query = term_query(fields.title, text);
    match substring_query_for_fields(text, &[fields.title]) {
        Some(substring_query) => Box::new(BooleanQuery::new(vec![
            (Occur::Should, exact_query),
            (Occur::Should, substring_query),
        ])),
        None => exact_query,
    }
}

fn field_expr_to_query(
    key: &str,
    value: &ValueExpr,
    fields: &SearchFields,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::AllQuery;

    let Some(field) = search_field_for_key(key, fields) else {
        return Box::new(AllQuery);
    };

    match value {
        ValueExpr::Text(text) if key == "title" =>
            title_field_query(text, fields),
        ValueExpr::Text(text) if key == "id" =>
            id_field_query(text, fields),
        ValueExpr::Text(text) => term_query(field, text),
        ValueExpr::Range { .. } => {
            if let Some(field_name) = tantivy_field_name_for_key(key) {
                build_range_query(field, field_name, fields, crate::model::query::CompareOp::Range, value)
            } else {
                Box::new(AllQuery)
            }
        }
        ValueExpr::Empty => Box::new(AllQuery),
    }
}

fn search_field_for_key(
    key: &str,
    fields: &SearchFields,
) -> Option<Field> {
    match key {
        "state" | "status" => Some(fields.state),
        "type" | "ticket_type" => Some(fields.ticket_type),
        "id" => Some(fields.id),
        "title" => Some(fields.title),
        "created" | "created_at" => Some(fields.created_at),
        "effort" => Some(fields.effort),
        _ => None,
    }
}

fn term_query(
    field: Field,
    text: &str,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::TermQuery;

    let term = Term::from_field_text(field, text);
    Box::new(TermQuery::new(term, Default::default()))
}

fn and_expr_to_query(
    exprs: &[Expr],
    fields: &SearchFields,
    index: &Index,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::{
        AllQuery,
        BooleanQuery,
        Occur,
    };

    if exprs.is_empty() {
        return Box::new(AllQuery);
    }

    let clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = exprs
        .iter()
        .map(|expr| (Occur::Must, expr_to_query(expr, fields, index)))
        .collect();
    Box::new(BooleanQuery::new(clauses))
}

fn or_expr_to_query(
    exprs: &[Expr],
    fields: &SearchFields,
    index: &Index,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::{
        AllQuery,
        BooleanQuery,
        Occur,
    };

    if exprs.is_empty() {
        return Box::new(AllQuery);
    }

    let clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = exprs
        .iter()
        .map(|expr| (Occur::Should, expr_to_query(expr, fields, index)))
        .collect();
    Box::new(BooleanQuery::new(clauses))
}

fn not_expr_to_query(
    expr: &Expr,
    fields: &SearchFields,
    index: &Index,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::{
        AllQuery,
        BooleanQuery,
        Occur,
    };

    Box::new(BooleanQuery::new(vec![
        (Occur::Must, Box::new(AllQuery)),
        (Occur::MustNot, expr_to_query(expr, fields, index)),
    ]))
}

fn truncate_snippet(
    text: &str,
    max_chars: usize,
) -> String {
    let mut s: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        s.push_str("…");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuilds_search_index_for_tantivy_thread_panic_messages() {
        let error = StorageError::SearchIndex(
            "storage error: search index error: An error occurred in a thread: 'Any { .. }'"
                .to_string(),
        );

        assert!(TantivySearchIndex::should_rebuild_search_index(&error));
    }

    #[test]
    fn rebuilds_search_index_for_index_out_of_bounds_panic() {
        let error = StorageError::SearchIndex(
            "index out of bounds: the len is 5 but the index is 5".to_string(),
        );

        assert!(TantivySearchIndex::should_rebuild_search_index(&error));
    }

    #[test]
    fn does_not_rebuild_search_index_for_retryable_permission_errors() {
        let error = StorageError::SearchIndex("permission denied".to_string());

        assert!(!TantivySearchIndex::should_rebuild_search_index(&error));
    }

    #[test]
    fn read_failure_rebuilds_for_any_non_transient_search_error() {
        // Segment-content damage surfaces with many different messages, none of
        // which the substring classifier covers — all must trigger a rebuild.
        for message in [
            "Failed to open file for read: FileDoesNotExist(\"seg.term\")",
            "File corrupted. The file is smaller than 4 bytes (len=2).",
            "UnexpectedEof while reading segment",
            "schema mismatch in segment",
        ] {
            let error = StorageError::SearchIndex(message.to_string());
            assert!(
                TantivySearchIndex::is_rebuildable_read_failure(&error),
                "expected rebuild for: {message}"
            );
        }
    }

    #[test]
    fn read_failure_does_not_rebuild_for_transient_permission_errors() {
        for message in [
            "PermissionDenied (os error 5)",
            "Access is denied. (os error 5)",
        ] {
            let error = StorageError::SearchIndex(message.to_string());
            assert!(
                !TantivySearchIndex::is_rebuildable_read_failure(&error),
                "expected no rebuild for transient: {message}"
            );
        }

        // Non-search errors are never rebuildable.
        assert!(!TantivySearchIndex::is_rebuildable_read_failure(
            &StorageError::DependencyCycle
        ));
    }

    #[test]
    fn open_or_create_heals_stale_fast_field_schema_on_construction() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("search_index");
        std::fs::create_dir_all(&index_dir).unwrap();

        // Build an OLDER 5-field schema (before `created_at`/`effort` fast
        // fields existed) so the on-disk layout no longer matches.
        let mut builder = Schema::builder();
        builder.add_text_field("id", STRING | STORED);
        builder.add_text_field("title", TEXT | STORED);
        builder.add_text_field("body", TEXT | STORED);
        builder.add_text_field("state", STRING | STORED | FAST);
        builder.add_text_field("ticket_type", STRING | STORED | FAST);
        Index::create_in_dir(&index_dir, builder.build()).unwrap();

        // Construction proactively validates and heals the stale schema, so the
        // index is already current before any operation runs.
        let search = TantivySearchIndex::open_or_create(&index_dir).unwrap();
        assert_eq!(search.check_invariants().unwrap(), None);

        // A write that references the newer fast fields must succeed (the
        // fast-field writer must not index past a stale field count).
        let id = Uuid::new_v4();
        search
            .upsert(
                &id,
                Some("title"),
                Some("searchable body"),
                Some("ready"),
                Some("rule-entry"),
                Some("2026-06-15T00:00:00Z"),
                Some("3"),
            )
            .unwrap();

        let expr = crate::model::query::parse_query("searchable").unwrap();
        assert_eq!(search.search(&expr, 10).unwrap().len(), 1);
    }

    #[test]
    fn check_invariants_detects_then_heals_stale_schema() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("search_index");

        // Start from a current, healthy index.
        let search = TantivySearchIndex::open_or_create(&index_dir).unwrap();
        assert_eq!(search.check_invariants().unwrap(), None);

        // Replace the on-disk index with an older 5-field schema behind the
        // existing handle (simulating a schema change across versions).
        std::fs::remove_dir_all(&index_dir).unwrap();
        std::fs::create_dir_all(&index_dir).unwrap();
        let mut builder = Schema::builder();
        builder.add_text_field("id", STRING | STORED);
        builder.add_text_field("title", TEXT | STORED);
        builder.add_text_field("body", TEXT | STORED);
        builder.add_text_field("state", STRING | STORED | FAST);
        builder.add_text_field("ticket_type", STRING | STORED | FAST);
        Index::create_in_dir(&index_dir, builder.build()).unwrap();

        // The read-only check detects the stale schema...
        assert_eq!(
            search.check_invariants().unwrap(),
            Some(IndexInvariant::SchemaCurrent)
        );

        // ...and the proactive gate heals it before any operation proceeds.
        search.ensure_schema_current().unwrap();
        assert_eq!(search.check_invariants().unwrap(), None);

        let id = Uuid::new_v4();
        search
            .upsert(&id, Some("t"), Some("healed token"), None, None, None, None)
            .unwrap();
        let expr = crate::model::query::parse_query("token").unwrap();
        assert_eq!(search.search(&expr, 10).unwrap().len(), 1);
    }

    #[test]
    fn check_invariants_detects_then_heals_corrupt_index() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("search_index");

        let search = TantivySearchIndex::open_or_create(&index_dir).unwrap();
        let id = Uuid::new_v4();
        search
            .upsert(&id, Some("before"), Some("before body"), None, None, None, None)
            .unwrap();

        // Corrupt the index metadata so it can no longer be opened.
        std::fs::write(index_dir.join("meta.json"), b"not valid json").unwrap();
        assert_eq!(
            search.check_invariants().unwrap(),
            Some(IndexInvariant::Openable)
        );

        // The next operation proactively rebuilds the index from the current
        // schema instead of erroring out.
        let id2 = Uuid::new_v4();
        search
            .upsert(&id2, Some("after"), Some("after token"), None, None, None, None)
            .unwrap();
        assert_eq!(search.check_invariants().unwrap(), None);

        let expr = crate::model::query::parse_query("token").unwrap();
        assert_eq!(search.search(&expr, 10).unwrap().len(), 1);
    }

    #[test]
    fn ensure_schema_current_keeps_current_schema_intact() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("search_index");

        let search = TantivySearchIndex::open_or_create(&index_dir).unwrap();
        let id = Uuid::new_v4();
        search
            .upsert(&id, Some("keep"), Some("keep body"), None, None, None, None)
            .unwrap();

        assert_eq!(search.check_invariants().unwrap(), None);
        // No-op when the schema already matches: the existing document survives.
        search.ensure_schema_current().unwrap();

        let expr = crate::model::query::parse_query("keep").unwrap();
        assert_eq!(search.search(&expr, 10).unwrap().len(), 1);
    }
}

