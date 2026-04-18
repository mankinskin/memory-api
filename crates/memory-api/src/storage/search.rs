use std::path::Path;

use tantivy::schema::{Field, Schema, Value as TantivyValue, FAST, STORED, STRING, TEXT};
use tantivy::{Index, IndexWriter, TantivyDocument, TantivyError, Term};
use uuid::Uuid;

use crate::error::StorageError;
use crate::model::query::{Expr, ValueExpr};

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
}

pub struct TantivySearchIndex {
    index: Index,
    fields: SearchFields,
}

impl TantivySearchIndex {
    pub fn open_or_create(dir: &Path) -> Result<Self, StorageError> {
        std::fs::create_dir_all(dir)?;

        let (schema, fields) = build_schema();

        let index = if dir
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
        {
            match Index::open_in_dir(dir) {
                Ok(idx) => idx,
                Err(_) => {
                    std::fs::remove_dir_all(dir)?;
                    std::fs::create_dir_all(dir)?;
                    Index::create_in_dir(dir, schema)
                        .map_err(|e| StorageError::SearchIndex(e.to_string()))?
                }
            }
        } else {
            Index::create_in_dir(dir, schema)
                .map_err(|e| StorageError::SearchIndex(e.to_string()))?
        };

        Ok(Self { index, fields })
    }

    fn writer(&self) -> Result<IndexWriter, StorageError> {
        self.index
            .writer(50_000_000)
            .map_err(|e| StorageError::SearchIndex(e.to_string()))
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
    ) -> Result<(), StorageError> {
        let mut writer = self.writer()?;
        writer.delete_term(Term::from_field_text(self.fields.id, &id.to_string()));

        let mut d = TantivyDocument::default();
        d.add_text(self.fields.id, id.to_string());
        if let Some(t) = title {
            d.add_text(self.fields.title, t);
        }
        if let Some(b) = body {
            d.add_text(self.fields.body, b);
        }
        if let Some(s) = state {
            d.add_text(self.fields.state, s);
        }
        if let Some(tp) = entity_type {
            d.add_text(self.fields.ticket_type, tp);
        }
        writer
            .add_document(d)
            .map_err(|e: TantivyError| StorageError::SearchIndex(e.to_string()))?;
        writer
            .commit()
            .map_err(|e| StorageError::SearchIndex(e.to_string()))?;
        Ok(())
    }

    pub fn remove(&self, id: &Uuid) -> Result<(), StorageError> {
        let mut writer = self.writer()?;
        writer.delete_term(Term::from_field_text(self.fields.id, &id.to_string()));
        writer
            .commit()
            .map_err(|e: TantivyError| StorageError::SearchIndex(e.to_string()))?;
        Ok(())
    }

    /// Delete every document from the Tantivy index.
    pub fn clear_all(&self) -> Result<(), StorageError> {
        let mut writer = self.writer()?;
        writer
            .delete_all_documents()
            .map_err(|e: TantivyError| StorageError::SearchIndex(e.to_string()))?;
        writer
            .commit()
            .map_err(|e: TantivyError| StorageError::SearchIndex(e.to_string()))?;
        Ok(())
    }

    /// Search using a parsed `Expr` AST.
    /// Returns up to `limit` results ordered by relevance score.
    pub fn search(&self, expr: &Expr, limit: usize) -> Result<Vec<SearchResult>, StorageError> {
        use tantivy::collector::TopDocs;
        use tantivy::query::{AllQuery, BooleanQuery, Occur, Query, TermQuery};

        let reader = self
            .index
            .reader()
            .map_err(|e| StorageError::SearchIndex(e.to_string()))?;
        let searcher = reader.searcher();

        let query: Box<dyn Query> = expr_to_query(expr, &self.fields, &self.index);

        let top_docs = searcher
            .search(&*query, &TopDocs::with_limit(limit))
            .map_err(|e| StorageError::SearchIndex(e.to_string()))?;

        let schema = self.index.schema();
        let mut results = Vec::with_capacity(top_docs.len());

        for (score, doc_addr) in top_docs {
            let doc = searcher
                .doc::<TantivyDocument>(doc_addr)
                .map_err(|e| StorageError::SearchIndex(e.to_string()))?;

            let id_str = get_text(&doc, self.fields.id, &schema);
            let id: Uuid = match id_str.as_deref().and_then(|s| s.parse().ok()) {
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
        let _ = (AllQuery, BooleanQuery::new(vec![]), Occur::Must, TermQuery::new(
            Term::from_field_text(self.fields.id, ""),
            Default::default(),
        ));

        Ok(results)
    }
}

fn build_schema() -> (Schema, SearchFields) {
    let mut builder = Schema::builder();
    let id = builder.add_text_field("id", STRING | STORED);
    let title = builder.add_text_field("title", TEXT | STORED);
    let body = builder.add_text_field("body", TEXT | STORED);
    let state = builder.add_text_field("state", STRING | STORED | FAST);
    let ticket_type = builder.add_text_field("ticket_type", STRING | STORED | FAST);
    let schema = builder.build();
    (schema, SearchFields { id, title, body, state, ticket_type })
}

fn get_text(doc: &TantivyDocument, field: Field, _schema: &Schema) -> Option<String> {
    doc.get_first(field)
        .and_then(|v| TantivyValue::as_str(&v))
        .map(str::to_string)
}

fn expr_to_query(
    expr: &Expr,
    fields: &SearchFields,
    index: &Index,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::{AllQuery, BooleanQuery, Occur, TermQuery};

    match expr {
        Expr::Fts(text) => {
            let mut qp =
                tantivy::query::QueryParser::for_index(index, vec![fields.title, fields.body]);
            qp.set_conjunction_by_default();
            match qp.parse_query(text) {
                Ok(q) => q,
                Err(_) => Box::new(AllQuery),
            }
        }
        Expr::Field { key, value } => {
            let field = match key.as_str() {
                "state" | "status" => fields.state,
                "type" | "ticket_type" => fields.ticket_type,
                "id" => fields.id,
                "title" => fields.title,
                _ => return Box::new(AllQuery),
            };
            match value {
                ValueExpr::Text(t) => {
                    let term = Term::from_field_text(field, t);
                    Box::new(TermQuery::new(term, Default::default()))
                }
                ValueExpr::Range { .. } => Box::new(AllQuery),
            }
        }
        Expr::And(exprs) => {
            if exprs.is_empty() {
                return Box::new(AllQuery);
            }
            let clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = exprs
                .iter()
                .map(|e| (Occur::Must, expr_to_query(e, fields, index)))
                .collect();
            Box::new(BooleanQuery::new(clauses))
        }
    }
}

fn truncate_snippet(text: &str, max_chars: usize) -> String {
    let mut s: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        s.push_str("…");
    }
    s
}
