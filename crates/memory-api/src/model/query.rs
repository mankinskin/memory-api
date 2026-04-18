use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::error::QueryParseError;

pub const DYNAMIC_FIELD_PREFIX: &str = "x_";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValueExpr {
    Text(String),
    Range { start: String, end: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Expr {
    And(Vec<Expr>),
    Fts(String),
    Field { key: String, value: ValueExpr },
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
        return Err(QueryParseError::InvalidExpression("query cannot be empty".to_string()));
    }

    let mut exprs = Vec::with_capacity(tokens.len());
    for token in tokens {
        if let Some((key, raw_value)) = token.split_once(':') {
            if key.is_empty() || raw_value.is_empty() {
                return Err(QueryParseError::InvalidExpression(format!(
                    "invalid field predicate: {token}"
                )));
            }

            if let Some(fields) = known_fields {
                validate_field_key(key, fields)?;
            }

            let value = if raw_value.starts_with('[') && raw_value.ends_with(']') && raw_value.contains(" TO ") {
                let inner = &raw_value[1..raw_value.len() - 1];
                let (start, end) = inner.split_once(" TO ").ok_or_else(|| {
                    QueryParseError::InvalidExpression(format!("invalid range expression: {token}"))
                })?;
                ValueExpr::Range {
                    start: start.to_string(),
                    end: end.to_string(),
                }
            } else {
                ValueExpr::Text(trim_quotes(raw_value))
            };

            exprs.push(Expr::Field {
                key: key.to_string(),
                value,
            });
        } else {
            exprs.push(Expr::Fts(trim_quotes(&token)));
        }
    }

    Ok(Expr::And(exprs))
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

    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}
