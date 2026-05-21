use crate::manifest::RuleManifest;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleFilter {
    pub state: Option<String>,
    pub file_kind: Option<String>,
    pub section: Option<String>,
    pub repo_scope: Option<String>,
    pub path_scope: Option<String>,
    pub slug: Option<String>,
    pub has_low_feedback: Option<bool>,
    pub has_unresolved_feedback: Option<bool>,
}

impl RuleFilter {
    pub(crate) fn matches(
        &self,
        rule: &RuleManifest,
    ) -> bool {
        self.matches_field(self.file_kind.as_deref(), rule.file_kind())
            && self.matches_field(self.section.as_deref(), rule.section())
            && self
                .matches_scope(self.repo_scope.as_deref(), rule.repo_scopes())
            && self
                .matches_scope(self.path_scope.as_deref(), rule.path_scopes())
            && self.matches_field(self.slug.as_deref(), rule.slug())
            && self.matches_low_feedback(rule)
            && self.matches_unresolved_feedback(rule)
    }

    fn matches_field(
        &self,
        expected: Option<&str>,
        actual: Option<&str>,
    ) -> bool {
        expected.is_none_or(|value| actual == Some(value))
    }

    fn matches_scope(
        &self,
        expected: Option<&str>,
        actual: Vec<String>,
    ) -> bool {
        expected.is_none_or(|value| actual.iter().any(|scope| scope == value))
    }

    fn matches_unresolved_feedback(
        &self,
        rule: &RuleManifest,
    ) -> bool {
        self.has_unresolved_feedback.is_none_or(|expected| {
            let unresolved =
                rule.feedback_unresolved_count().unwrap_or_default() > 0;
            unresolved == expected
        })
    }

    fn matches_low_feedback(
        &self,
        rule: &RuleManifest,
    ) -> bool {
        self.has_low_feedback.is_none_or(|expected| {
            let has_low_feedback =
                rule.feedback_mixed_count().unwrap_or_default() > 0
                    || rule.feedback_not_helpful_count().unwrap_or_default()
                        > 0;
            has_low_feedback == expected
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuleManifest;

    #[test]
    fn matches_low_feedback_for_mixed_and_not_helpful_rules() {
        let mut rule = RuleManifest::new(
            "shared/agents/feedback",
            "Feedback",
            "AGENTS",
            "feedback",
            "body",
        );
        rule.set_feedback_summary(0, 1, 0, 0, 0, Some("2026-05-12T00:00:00Z"));

        assert!(
            RuleFilter {
                has_low_feedback: Some(true),
                ..RuleFilter::default()
            }
            .matches(&rule)
        );
        assert!(
            !RuleFilter {
                has_low_feedback: Some(false),
                ..RuleFilter::default()
            }
            .matches(&rule)
        );
    }
}
