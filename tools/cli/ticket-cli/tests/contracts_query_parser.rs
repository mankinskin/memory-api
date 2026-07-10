use std::collections::BTreeSet;
use ticket_api::model::query::{
    CompareOp,
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
    let expr = parse_query("status:open -assigned:bob").expect("query parses");

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
    let err =
        parse_query("status:open OR").expect_err("dangling OR should fail");
    assert!(
        err.to_string()
            .contains("OR must separate two query expressions")
    );
}

#[test]
fn parse_plain_equality_stays_field_for_backward_compat() {
    let expr = parse_query("status:open").expect("query parses");
    match expr {
        Expr::And(parts) => assert!(matches!(
            parts[0],
            Expr::Field { ref key, value: ValueExpr::Text(ref v) }
            if key == "status" && v == "open"
        )),
        _ => panic!("expected Expr::And"),
    }
}

#[test]
fn parse_contains_operator_tilde_and_star_aliases() {
    for token in ["title:~login", "title:*login*"] {
        let expr = parse_query(token).expect("contains query parses");
        match expr {
            Expr::And(parts) => assert!(
                matches!(
                    parts[0],
                    Expr::Compare {
                        ref key,
                        op: CompareOp::Contains,
                        value: ValueExpr::Text(ref v),
                    }
                    if key == "title" && v == "login"
                ),
                "unexpected expr for {token}"
            ),
            _ => panic!("expected Expr::And for {token}"),
        }
    }
}

#[test]
fn parse_comparison_operators_pick_longest_prefix() {
    let cases = [
        ("created:>=2026-01-01", CompareOp::Gte, "2026-01-01"),
        ("created:<=2026-12-31", CompareOp::Lte, "2026-12-31"),
        ("created:>2026-01-01", CompareOp::Gt, "2026-01-01"),
        ("created:<2026-12-31", CompareOp::Lt, "2026-12-31"),
    ];
    for (token, want_op, want_val) in cases {
        let expr = parse_query(token).expect("comparison parses");
        match expr {
            Expr::And(parts) => match &parts[0] {
                Expr::Compare { key, op, value } => {
                    assert_eq!(key, "created", "key for {token}");
                    assert_eq!(*op, want_op, "op for {token}");
                    assert!(
                        matches!(value, ValueExpr::Text(v) if v == want_val)
                    );
                },
                other => panic!("expected Compare for {token}, got {other:?}"),
            },
            _ => panic!("expected Expr::And for {token}"),
        }
    }
}

#[test]
fn parse_exists_predicate() {
    let expr = parse_query("assigned:?").expect("exists query parses");
    match expr {
        Expr::And(parts) => assert!(matches!(
            parts[0],
            Expr::Compare {
                ref key,
                op: CompareOp::Exists,
                value: ValueExpr::Empty,
            }
            if key == "assigned"
        )),
        _ => panic!("expected Expr::And"),
    }
}

#[test]
fn parse_negated_exists_is_not_over_exists() {
    let expr = parse_query("-assigned:?").expect("negated exists parses");
    match expr {
        Expr::And(parts) => match &parts[0] {
            Expr::Not(inner) => assert!(matches!(
                **inner,
                Expr::Compare {
                    op: CompareOp::Exists,
                    ..
                }
            )),
            other => panic!("expected Not(Exists), got {other:?}"),
        },
        _ => panic!("expected Expr::And"),
    }
}

#[test]
fn parse_range_stays_field_range() {
    let expr = parse_query("created:[2026-01-01 TO 2026-12-31]")
        .expect("range parses");
    match expr {
        Expr::And(parts) => match &parts[0] {
            Expr::Field {
                key,
                value: ValueExpr::Range { start, end },
            } => {
                assert_eq!(key, "created");
                assert_eq!(start, "2026-01-01");
                assert_eq!(end, "2026-12-31");
            },
            other => panic!("expected Field Range, got {other:?}"),
        },
        _ => panic!("expected Expr::And"),
    }
}

#[test]
fn parse_dotted_dynamic_path_normalizes_to_flat_key() {
    let expr =
        parse_query("x.feature.story_points:8").expect("deep path parses");
    match expr {
        Expr::And(parts) => assert!(matches!(
            parts[0],
            Expr::Field { ref key, value: ValueExpr::Text(ref v) }
            if key == "x_feature_story_points" && v == "8"
        )),
        _ => panic!("expected Expr::And"),
    }
}

#[test]
fn parse_dotted_dynamic_path_validates_in_strict_mode() {
    let known = BTreeSet::from(["status".to_string()]);
    let expr = parse_query_strict("x.feature.points:3", &known)
        .expect("normalized dynamic path should pass strict validation");
    match expr {
        Expr::And(parts) => assert!(matches!(
            parts[0],
            Expr::Field { ref key, .. } if key == "x_feature_points"
        )),
        _ => panic!("expected Expr::And"),
    }
}

#[test]
fn parse_comparison_missing_value_fails() {
    let err = parse_query("created:>").expect_err("dangling comparison fails");
    assert!(err.to_string().contains("missing a value"));
}
