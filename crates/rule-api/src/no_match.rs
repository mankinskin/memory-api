#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MissingRuleMatchSignal {
    pub query: String,
    pub context_tags: Vec<String>,
    pub has_matching_rule: bool,
}

impl MissingRuleMatchSignal {
    pub fn indicates_no_match(&self) -> bool {
        !self.has_matching_rule
    }
}

pub fn emit_missing_rule_match_signal(
    query: impl Into<String>,
    context_tags: &[String],
    has_matching_rule: bool,
) -> Option<MissingRuleMatchSignal> {
    if has_matching_rule {
        return None;
    }

    Some(MissingRuleMatchSignal {
        query: query.into(),
        context_tags: context_tags.to_vec(),
        has_matching_rule,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_signal_only_when_no_rule_matches() {
        let context_tags = vec!["policy".to_string()];

        let no_match =
            emit_missing_rule_match_signal("query", &context_tags, false)
                .expect("signal");
        assert!(no_match.indicates_no_match());
        assert_eq!(no_match.query, "query");

        assert!(
            emit_missing_rule_match_signal("query", &context_tags, true)
                .is_none()
        );
    }
}
