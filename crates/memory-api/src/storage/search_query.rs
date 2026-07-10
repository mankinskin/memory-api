use super::*;

pub(super) fn schemas_match(
    a: &Schema,
    b: &Schema,
) -> bool {
    match (serde_json::to_string(a), serde_json::to_string(b)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub(super) fn build_schema() -> (Schema, SearchFields) {
    let mut builder = Schema::builder();
    let id = builder.add_text_field("id", STRING | STORED);
    let title = builder.add_text_field("title", TEXT | STORED);
    let body = builder.add_text_field("body", TEXT | STORED);
    let state = builder.add_text_field("state", STRING | STORED | FAST);
    let ticket_type =
        builder.add_text_field("ticket_type", STRING | STORED | FAST);
    let created_at =
        builder.add_text_field("created_at", STRING | STORED | FAST);
    let effort = builder.add_i64_field("effort", INDEXED | STORED | FAST);
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

pub(super) fn get_text(
    doc: &TantivyDocument,
    field: Field,
    _schema: &Schema,
) -> Option<String> {
    doc.get_first(field)
        .and_then(|v| TantivyValue::as_str(&v))
        .map(str::to_string)
}

pub(super) fn expr_to_query(
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
pub(super) fn tantivy_field_name_for_key(key: &str) -> Option<&str> {
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

pub(super) fn compare_expr_to_query(
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
        (CompareOp::Gt, _)
        | (CompareOp::Gte, _)
        | (CompareOp::Lt, _)
        | (CompareOp::Lte, _)
        | (CompareOp::Range, _) =>
            build_range_query(field, field_name, fields, op, value),
        _ => Box::new(AllQuery),
    }
}

pub(super) fn build_range_query(
    field: Field,
    field_name: &str,
    fields: &SearchFields,
    op: crate::model::query::CompareOp,
    value: &ValueExpr,
) -> Box<dyn tantivy::query::Query> {
    use crate::model::query::CompareOp;
    use std::ops::Bound;
    use tantivy::query::{
        EmptyQuery,
        RangeQuery,
    };

    let get_term = |val_str: &str| -> Option<Term> {
        if field == fields.effort {
            val_str
                .parse::<i64>()
                .ok()
                .map(|val| Term::from_field_i64(field, val))
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
        },
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
        },
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
        },
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
        },
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
        },
        _ => Box::new(EmptyQuery),
    }
}

/// Match any document that has a non-null value in `field`.
pub(super) fn exists_query(field: Field) -> Box<dyn tantivy::query::Query> {
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

pub(super) fn full_text_query(
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

pub(super) fn substring_query_for_fields(
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

pub(super) fn id_field_query(
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

pub(super) fn title_field_query(
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

pub(super) fn field_expr_to_query(
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
        ValueExpr::Text(text) if key == "id" => id_field_query(text, fields),
        ValueExpr::Text(text) => term_query(field, text),
        ValueExpr::Range { .. } => {
            if let Some(field_name) = tantivy_field_name_for_key(key) {
                build_range_query(
                    field,
                    field_name,
                    fields,
                    crate::model::query::CompareOp::Range,
                    value,
                )
            } else {
                Box::new(AllQuery)
            }
        },
        ValueExpr::Empty => Box::new(AllQuery),
    }
}

pub(super) fn search_field_for_key(
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

pub(super) fn term_query(
    field: Field,
    text: &str,
) -> Box<dyn tantivy::query::Query> {
    use tantivy::query::TermQuery;

    let term = Term::from_field_text(field, text);
    Box::new(TermQuery::new(term, Default::default()))
}

pub(super) fn and_expr_to_query(
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

pub(super) fn or_expr_to_query(
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

pub(super) fn not_expr_to_query(
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
