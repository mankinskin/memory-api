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

const SEARCH_IO_RETRY_ATTEMPTS: usize = 8;
const SEARCH_IO_RETRY_BASE_DELAY_MS: u64 = 25;

impl TantivySearchIndex {
    pub fn open_or_create(dir: &Path) -> Result<Self, StorageError> {
        std::fs::create_dir_all(dir)?;

        let (schema, fields) = build_schema();

        // Open (or create) the index once to validate the directory, then
        // immediately drop it.  We do NOT keep it open so that Windows
        // MmapDirectory handles are released between operations.
        open_or_create_index(dir, schema)?;

        Ok(Self {
            dir: dir.to_path_buf(),
            fields,
        })
    }

    /// Open a fresh `Index` handle for a single operation, then drop it.
    fn open_index(&self) -> Result<Index, StorageError> {
        let (schema, _) = build_schema();
        open_or_create_index(&self.dir, schema)
    }

    fn make_writer(index: &Index) -> Result<IndexWriter, StorageError> {
        index
            .writer(50_000_000)
            .map_err(|e| StorageError::SearchIndex(e.to_string()))
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

    fn should_rebuild_search_index(error: &StorageError) -> bool {
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

    fn rebuild_search_index_dir(&self) -> Result<(), StorageError> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        std::fs::create_dir_all(&self.dir)?;
        Ok(())
    }

    fn with_search_index_recovery<T, F>(&self, mut op: F) -> Result<T, StorageError>
    where
        F: FnMut() -> Result<T, StorageError>,
    {
        match op() {
            Ok(value) => Ok(value),
            Err(error) if Self::should_rebuild_search_index(&error) => {
                self.rebuild_search_index_dir()?;
                op()
            }
            Err(error) => Err(error),
        }
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
        self.with_search_index_recovery(|| {
            Self::with_retry(|| {
                let index = self.open_index()?;
                let mut writer = Self::make_writer(&index)?;
                writer.delete_term(Term::from_field_text(
                    self.fields.id,
                    &id.to_string(),
                ));

                let mut d = TantivyDocument::default();
                d.add_text(self.fields.id, id.to_string());
                if let Some(t) = title {
                    d.add_text(self.fields.title, t);
                }
                if let Some(b) = body {
                    d.add_text(self.fields.body, b);
                }
                d.add_text(self.fields.state, state.unwrap_or(""));
                d.add_text(self.fields.ticket_type, entity_type.unwrap_or(""));
                d.add_text(self.fields.created_at, created_at.unwrap_or(""));
                let eff_val = effort
                    .and_then(|eff_str| eff_str.parse::<i64>().ok())
                    .unwrap_or(0);
                d.add_i64(self.fields.effort, eff_val);
                writer.add_document(d).map_err(|e: TantivyError| {
                    StorageError::SearchIndex(e.to_string())
                })?;
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
        })
    }

    pub fn remove(
        &self,
        id: &Uuid,
    ) -> Result<(), StorageError> {
        self.with_search_index_recovery(|| {
            Self::with_retry(|| {
                let index = self.open_index()?;
                let mut writer = Self::make_writer(&index)?;
                writer.delete_term(Term::from_field_text(
                    self.fields.id,
                    &id.to_string(),
                ));
                writer.commit().map_err(|e: TantivyError| {
                    StorageError::SearchIndex(e.to_string())
                })?;
                writer
                    .wait_merging_threads()
                    .map_err(|e| StorageError::SearchIndex(e.to_string()))?;
                drop(index);
                Ok(())
            })
        })
    }

    /// Delete every document from the Tantivy index.
    pub fn clear_all(&self) -> Result<(), StorageError> {
        self.with_search_index_recovery(|| {
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
        })
    }

    /// Search using a parsed `Expr` AST.
    /// Returns up to `limit` results ordered by relevance score.
    pub fn search(
        &self,
        expr: &Expr,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        self.with_search_index_recovery(|| self.search_once(expr, limit))
    }

    fn search_once(
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
    }
}

/// Open the Tantivy index at `dir`, or create it from `schema` if the
/// directory is empty.  If the directory is non-empty but the index cannot be
/// opened **or** opens into an unreadable state (e.g. corrupt `meta.json` or a
/// missing/corrupt segment file referenced by `meta.json`), the directory is
/// wiped and recreated so every access self-heals.
fn open_or_create_index(
    dir: &Path,
    schema: Schema,
) -> Result<Index, StorageError> {
    let has_existing = dir
        .read_dir()
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);

    if has_existing {
        // Open and probe the existing index. `Index::open_in_dir` can succeed on
        // an index whose `meta.json` is valid but whose segment files are
        // missing or corrupt, so a successful open is not enough — we must also
        // confirm the index is actually readable before reusing it.
        let opened = Index::open_in_dir(dir);
        let reusable = match &opened {
            Ok(index) => index_is_readable(index),
            Err(_) => false,
        };

        if reusable {
            return opened
                .map_err(|e| StorageError::SearchIndex(e.to_string()));
        }

        // Drop any handles from the failed/unreadable open before wiping. On
        // Windows MmapDirectory holds file handles that would block removal.
        drop(opened);
        return recreate_index_dir(dir, schema);
    }

    Index::create_in_dir(dir, schema)
        .map_err(|e| StorageError::SearchIndex(e.to_string()))
}

/// Probe whether an opened index can actually be read. Running a trivial
/// match-all count forces Tantivy to open the segment readers referenced by
/// `meta.json`, so this detects missing or corrupt segment files that a bare
/// open would miss.
fn index_is_readable(index: &Index) -> bool {
    use tantivy::{
        collector::Count,
        query::AllQuery,
    };

    let Ok(reader) = index.reader() else {
        return false;
    };
    reader.searcher().search(&AllQuery, &Count).is_ok()
}

/// Wipe and recreate the index directory from `schema`.
fn recreate_index_dir(
    dir: &Path,
    schema: Schema,
) -> Result<Index, StorageError> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::create_dir_all(dir)?;
    Index::create_in_dir(dir, schema)
        .map_err(|e| StorageError::SearchIndex(e.to_string()))
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
    use tempfile::tempdir;

    fn sample_id(byte: u8) -> Uuid {
        Uuid::from_bytes([byte; 16])
    }

    fn match_all() -> Expr {
        // An empty `And` lowers to a Tantivy `AllQuery`.
        Expr::And(Vec::new())
    }

    fn seed_index(index: &TantivySearchIndex, id: &Uuid, title: &str) {
        index
            .upsert(
                id,
                Some(title),
                Some("body text"),
                Some("ready"),
                Some("tracker-improvement"),
                Some("2026-06-06T00:00:00Z"),
                Some("3"),
            )
            .expect("seed upsert should succeed on a healthy index");
    }

    /// Replace `meta.json` with garbage so the index cannot be opened at all.
    fn corrupt_meta(dir: &Path) {
        std::fs::write(dir.join("meta.json"), b"not valid json")
            .expect("should be able to overwrite meta.json");
    }

    /// Delete the segment payload files while leaving `meta.json` intact, so the
    /// index opens successfully but is structurally unreadable.
    fn delete_segment_files(dir: &Path) {
        for entry in std::fs::read_dir(dir).expect("index dir should be listable")
        {
            let path = entry.expect("dir entry should be readable").path();
            let is_meta = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == "meta.json")
                .unwrap_or(false);
            let is_lock = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "lock")
                .unwrap_or(false);
            if path.is_file() && !is_meta && !is_lock {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

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
    fn open_rejects_index_with_corrupt_meta_and_recreates() {
        let dir = tempdir().unwrap();
        let index = TantivySearchIndex::open_or_create(dir.path()).unwrap();
        seed_index(&index, &sample_id(1), "first");
        corrupt_meta(dir.path());

        // Opening again must not error — it self-heals into a fresh index.
        let reopened = TantivySearchIndex::open_or_create(dir.path()).unwrap();
        let results = reopened.search(&match_all(), 10).unwrap();
        assert!(
            results.is_empty(),
            "corrupt index should be rebuilt empty, got {results:?}"
        );
    }

    #[test]
    fn upsert_self_heals_when_meta_is_corrupt() {
        let dir = tempdir().unwrap();
        let index = TantivySearchIndex::open_or_create(dir.path()).unwrap();
        seed_index(&index, &sample_id(1), "first");
        corrupt_meta(dir.path());

        // A write against the invalid index must succeed by rebuilding.
        seed_index(&index, &sample_id(2), "second");

        let results = index.search(&match_all(), 10).unwrap();
        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        assert_eq!(
            ids,
            vec![sample_id(2)],
            "write after corruption should self-heal and retain the new doc"
        );
    }

    #[test]
    fn search_self_heals_when_segments_are_missing() {
        let dir = tempdir().unwrap();
        let index = TantivySearchIndex::open_or_create(dir.path()).unwrap();
        seed_index(&index, &sample_id(1), "first");
        delete_segment_files(dir.path());

        // The read path must self-heal instead of erroring.
        let results = index.search(&match_all(), 10).unwrap();
        assert!(
            results.is_empty(),
            "search against an unreadable index should rebuild and return empty"
        );

        // After self-heal the index is usable again for writes + reads.
        seed_index(&index, &sample_id(2), "second");
        let ids: Vec<Uuid> = index
            .search(&match_all(), 10)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec![sample_id(2)]);
    }

    #[test]
    fn remove_self_heals_when_meta_is_corrupt() {
        let dir = tempdir().unwrap();
        let index = TantivySearchIndex::open_or_create(dir.path()).unwrap();
        seed_index(&index, &sample_id(1), "first");
        corrupt_meta(dir.path());

        index
            .remove(&sample_id(1))
            .expect("remove against an invalid index should self-heal");

        assert!(index.search(&match_all(), 10).unwrap().is_empty());
    }

    #[test]
    fn clear_all_self_heals_when_meta_is_corrupt() {
        let dir = tempdir().unwrap();
        let index = TantivySearchIndex::open_or_create(dir.path()).unwrap();
        seed_index(&index, &sample_id(1), "first");
        corrupt_meta(dir.path());

        index
            .clear_all()
            .expect("clear_all against an invalid index should self-heal");

        assert!(index.search(&match_all(), 10).unwrap().is_empty());
    }
}
