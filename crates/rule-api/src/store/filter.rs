use crate::manifest::RuleManifest;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleFilter {
    pub state: Option<String>,
    pub file_kind: Option<String>,
    pub section: Option<String>,
    pub repo_scope: Option<String>,
    pub path_scope: Option<String>,
    pub slug: Option<String>,
    pub has_unresolved_feedback: Option<bool>,
}

impl RuleFilter {
    pub(crate) fn matches(&self, rule: &RuleManifest) -> bool {
        self.matches_field(self.file_kind.as_deref(), rule.file_kind())
            && self.matches_field(self.section.as_deref(), rule.section())
            && self.matches_scope(self.repo_scope.as_deref(), rule.repo_scopes())
            && self.matches_scope(self.path_scope.as_deref(), rule.path_scopes())
            && self.matches_field(self.slug.as_deref(), rule.slug())
            && self.matches_unresolved_feedback(rule)
    }

    fn matches_field(&self, expected: Option<&str>, actual: Option<&str>) -> bool {
        expected.is_none_or(|value| actual == Some(value))
    }

    fn matches_scope(&self, expected: Option<&str>, actual: Vec<String>) -> bool {
        expected.is_none_or(|value| actual.iter().any(|scope| scope == value))
    }

    fn matches_unresolved_feedback(&self, rule: &RuleManifest) -> bool {
        self.has_unresolved_feedback.is_none_or(|expected| {
            let unresolved = rule.feedback_unresolved_count().unwrap_or_default() > 0;
            unresolved == expected
        })
    }
}