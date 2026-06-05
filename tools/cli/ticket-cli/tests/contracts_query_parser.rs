use std::collections::BTreeSet;
use ticket_api::model::query::{
    Expr,
    ValueExpr,
    parse_query,
    parse_query_strict,
};

#[test]
fn parse_mixed_fts_and_fields() {
    let expr = parse_query("status:open assigned:alice \"login page\"")
        .expect("query parses");

    match expr {
        Expr::And(parts) => {
            assert_eq!(parts.len(), 3);
            assert!(matches!(
                parts[0],
                Expr::Field { ref key, value: ValueExpr::Text(ref v) }
                if key == "status" && v == "open"
            ));
            assert!(matches!(
                parts[1],
                Expr::Field { ref key, value: ValueExpr::Text(ref v) }
                if key == "assigned" && v == "alice"
            ));
            assert!(matches!(parts[2], Expr::Fts(ref v) if v == "login page"));
        },
        _ => panic!("expected Expr::And"),
    }
}

#[test]
fn parse_empty_query_fails() {
    let err = parse_query("   ").expect_err("empty query should fail");
    assert!(err.to_string().contains("query cannot be empty"));
}

#[test]
fn strict_parser_rejects_unknown_field_with_deterministic_hint() {
    let known = BTreeSet::from([
        "assigned".to_string(),
        "created".to_string(),
        "status".to_string(),
    ]);

    let err = parse_query_strict("priority:high", &known)
        .expect_err("unknown field should fail in strict mode");

    let message = err.to_string();
    assert!(message.contains("unknown field 'priority'"));
    assert!(message.contains("Hint:"));
    assert!(message.contains("x_<type>_<field>"));
}

#[test]
fn strict_parser_allows_dynamic_namespaced_field() {
    let known = BTreeSet::from([
        "assigned".to_string(),
        "created".to_string(),
        "status".to_string(),
    ]);

    let expr =
        parse_query_strict("x_feature_story_points:8 status:open", &known)
            .expect("dynamic namespaced field should be allowed");

    match expr {
        Expr::And(parts) => assert_eq!(parts.len(), 2),
        _ => panic!("expected Expr::And"),
    }
}

#[test]
fn parse_or_groups_into_disjunction_of_and_clauses() {
    let expr = parse_query("status:open OR assigned:alice \"login page\"")
        .expect("query parses");

    match expr {
        Expr::Or(groups) => {
            assert_eq!(groups.len(), 2);
            assert!(matches!(&groups[0], Expr::And(parts) if parts.len() == 1));
            assert!(matches!(&groups[1], Expr::And(parts) if parts.len() == 2));
        },
        _ => panic!("expected Expr::Or"),
    }
}

#[test]
fn parse_dash_prefix_as_not_expression() {
    let expr = parse_query("status:open -assigned:bob")
        .expect("query parses");

    match expr {
        Expr::And(parts) => {
            assert_eq!(parts.len(), 2);
            assert!(matches!(parts[1], Expr::Not(_)));
        },
        _ => panic!("expected Expr::And"),
    }
}

#[test]
fn parse_or_without_rhs_fails() {
    let err = parse_query("status:open OR")
        .expect_err("dangling OR should fail");
    assert!(err.to_string().contains("OR must separate two query expressions"));
}
