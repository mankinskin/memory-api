use serde::{
    Deserialize,
    Serialize,
};
use std::collections::BTreeSet;

use crate::error::QueryParseError;

pub const DYNAMIC_FIELD_PREFIX: &str = "x_";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValueExpr {
    Text(String),
    Range {
        start: String,
        end: String,
    },
    /// Value-less marker used by the existence predicate (`key:?`).
    Empty,
}

/// Comparison operator for a field predicate.
///
/// `Eq` is the canonical equality form and is also represented by the legacy
/// [`Expr::Field`] variant for backward compatibility; the parser keeps
/// emitting [`Expr::Field`] for plain `key:value` so existing consumers and
/// tests remain valid. All other operators are emitted as [`Expr::Compare`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompareOp {
    /// Exact match: `key:value`.
    Eq,
    /// Substring match on the field's text value: `key:~value` or `key:*value*`.
    Contains,
    /// Strictly greater than: `key:>value`.
    Gt,
    /// Greater than or equal: `key:>=value`.
    Gte,
    /// Strictly less than: `key:<value`.
    Lt,
    /// Less than or equal: `key:<=value`.
    Lte,
    /// Inclusive range: `key:[a TO b]`.
    Range,
    /// Field present and non-empty: `key:?`.
    Exists,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Expr {
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
    Fts(String),
    /// Legacy equality / range field predicate (`key:value`, `key:[a TO b]`).
    ///
    /// Retained as the canonical `Eq`/`Range` shape so existing parser output
    /// and consumers keep working. New comparison operators are emitted via
    /// [`Expr::Compare`].
    Field {
        key: String,
        value: ValueExpr,
    },
    /// Field predicate carrying an explicit comparison operator and a
    /// (possibly deep / dotted) field path.
    Compare {
        key: String,
        op: CompareOp,
        value: ValueExpr,
    },
}

pub fn parse_query(input: &str) -> Result<Expr, QueryParseError> {
    parse_query_internal(input, None)
}

/// Strict parsing mode used by contract validation.
///
/// Rules:
/// - keys in `known_fields` are always valid
/// - dynamic keys must follow `x_<type>_<field>`
/// - unknown keys fail with deterministic hint text
pub fn parse_query_strict(
    input: &str,
    known_fields: &BTreeSet<String>,
) -> Result<Expr, QueryParseError> {
    parse_query_internal(input, Some(known_fields))
}

fn parse_query_internal(
    input: &str,
    known_fields: Option<&BTreeSet<String>>,
) -> Result<Expr, QueryParseError> {
    let tokens = tokenize(input);
    if tokens.is_empty() {
        return Err(QueryParseError::InvalidExpression(
            "query cannot be empty".to_string(),
        ));
    }

    let mut groups: Vec<Vec<Expr>> = vec![Vec::new()];
    for token in tokens {
        if token.eq_ignore_ascii_case("OR") {
            groups.push(Vec::new());
            continue;
        }

        let expr = parse_token(&token, known_fields)?;
        groups
            .last_mut()
            .expect("query groups should always have a current group")
            .push(expr);
    }

    if groups.iter().any(Vec::is_empty) {
        return Err(QueryParseError::InvalidExpression(
            "OR must separate two query expressions".to_string(),
        ));
    }

    if groups.len() == 1 {
        Ok(Expr::And(groups.pop().unwrap_or_default()))
    } else {
        Ok(Expr::Or(groups.into_iter().map(Expr::And).collect()))
    }
}

fn parse_token(
    token: &str,
    known_fields: Option<&BTreeSet<String>>,
) -> Result<Expr, QueryParseError> {
    let (negated, raw_token) = match token.strip_prefix('-') {
        Some(rest) if !rest.is_empty() => (true, rest),
        _ => (false, token),
    };

    let expr = if let Some((raw_key, raw_value)) = raw_token.split_once(':') {
        if raw_key.is_empty() || raw_value.is_empty() {
            return Err(QueryParseError::InvalidExpression(format!(
                "invalid field predicate: {raw_token}"
            )));
        }

        // Normalize dotted deep-field addressing (`x.<type>.<field>`) to the
        // canonical flat dynamic key (`x_<type>_<field>`) so storage/index
        // keys stay stable. Non-dynamic dotted paths are left untouched.
        let key = normalize_field_path(raw_key);

        if let Some(fields) = known_fields {
            validate_field_key(&key, fields)?;
        }

        let (op, value) = parse_field_value(&key, raw_value, raw_token)?;

        match op {
            CompareOp::Eq => Expr::Field { key, value },
            CompareOp::Range => Expr::Field { key, value },
            _ => Expr::Compare { key, op, value },
        }
    } else {
        Expr::Fts(trim_quotes(raw_token))
    };

    if negated {
        Ok(Expr::Not(Box::new(expr)))
    } else {
        Ok(expr)
    }
}

/// Normalize a raw field path token to the canonical storage key.
///
/// Dotted dynamic addressing `x.<type>.<field>` collapses to the flat
/// `x_<type>_<field>` form. Any other path (including plain keys without dots
/// and non-`x` dotted paths) is returned unchanged.
fn normalize_field_path(raw_key: &str) -> String {
    if let Some(rest) = raw_key.strip_prefix("x.") {
        if !rest.is_empty() && rest.contains('.') {
            return format!("{DYNAMIC_FIELD_PREFIX}{}", rest.replace('.', "_"));
        }
    }
    raw_key.to_string()
}

/// Parse the comparison operator and value from the raw value side of a
/// `key:<value>` token.
fn parse_field_value(
    key: &str,
    raw_value: &str,
    raw_token: &str,
) -> Result<(CompareOp, ValueExpr), QueryParseError> {
    if let Some(parsed) = parse_special_field_value(raw_value, raw_token)? {
        return Ok(parsed);
    }

    let (op, rest) = parse_compare_prefix(raw_value);

    if rest.is_empty() {
        return Err(QueryParseError::InvalidExpression(format!(
            "comparison operator on field '{key}' is missing a value: {raw_token}"
        )));
    }

    Ok((op, ValueExpr::Text(trim_quotes(rest))))
}

fn parse_special_field_value(
    raw_value: &str,
    raw_token: &str,
) -> Result<Option<(CompareOp, ValueExpr)>, QueryParseError> {
    if raw_value == "?" {
        return Ok(Some((CompareOp::Exists, ValueExpr::Empty)));
    }

    if raw_value.starts_with('[')
        && raw_value.ends_with(']')
        && raw_value.contains(" TO ")
    {
        let inner = &raw_value[1..raw_value.len() - 1];
        let (start, end) = inner.split_once(" TO ").ok_or_else(|| {
            QueryParseError::InvalidExpression(format!(
                "invalid range expression: {raw_token}"
            ))
        })?;
        return Ok(Some((
            CompareOp::Range,
            ValueExpr::Range {
                start: start.trim().to_string(),
                end: end.trim().to_string(),
            },
        )));
    }

    Ok(None)
}

fn parse_compare_prefix(raw_value: &str) -> (CompareOp, &str) {
    if let Some(rest) = raw_value.strip_prefix(">=") {
        (CompareOp::Gte, rest)
    } else if let Some(rest) = raw_value.strip_prefix("<=") {
        (CompareOp::Lte, rest)
    } else if let Some(rest) = raw_value.strip_prefix('>') {
        (CompareOp::Gt, rest)
    } else if let Some(rest) = raw_value.strip_prefix('<') {
        (CompareOp::Lt, rest)
    } else if let Some(rest) = raw_value.strip_prefix('~') {
        (CompareOp::Contains, rest)
    } else if raw_value.len() >= 2
        && raw_value.starts_with('*')
        && raw_value.ends_with('*')
    {
        (CompareOp::Contains, &raw_value[1..raw_value.len() - 1])
    } else {
        (CompareOp::Eq, raw_value)
    }
}

fn validate_field_key(
    key: &str,
    known_fields: &BTreeSet<String>,
) -> Result<(), QueryParseError> {
    if known_fields.contains(key) {
        return Ok(());
    }

    if is_valid_dynamic_field_key(key) {
        return Ok(());
    }

    let hint = known_fields
        .iter()
        .next()
        .map(std::string::String::as_str)
        .unwrap_or("status");

    Err(QueryParseError::InvalidExpression(format!(
        "unknown field '{key}'. Hint: use known fields or dynamic namespace '{DYNAMIC_FIELD_PREFIX}<type>_<field>' (e.g. {hint}:open)"
    )))
}

pub fn is_valid_dynamic_field_key(key: &str) -> bool {
    if !key.starts_with(DYNAMIC_FIELD_PREFIX) {
        return false;
    }
    let mut parts = key.split('_');
    let p0 = parts.next();
    let p1 = parts.next();
    let p2 = parts.next();
    p0 == Some("x")
        && p1.is_some_and(|p| !p.is_empty())
        && p2.is_some_and(|p| !p.is_empty())
}

fn trim_quotes(s: &str) -> String {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut bracket_depth: usize = 0;

    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            },
            // Suppress splitting inside a `[a TO b]` range so the embedded
            // space does not break the token apart.
            '[' if !in_quotes => {
                bracket_depth += 1;
                current.push(ch);
            },
            ']' if !in_quotes && bracket_depth > 0 => {
                bracket_depth -= 1;
                current.push(ch);
            },
            c if c.is_whitespace() && !in_quotes && bracket_depth == 0 =>
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                },
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}
