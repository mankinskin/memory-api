use std::collections::HashSet;

use feedback_api::{
    EntityFeedbackStore,
    EntityUrn,
    FeedbackEntry,
    FeedbackNoteKind,
    FeedbackProvenance,
    FeedbackRating,
    FeedbackSource,
    IngestAuthor,
};

use crate::{
    SessionRole,
    SessionTurn,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MiningFeedbackResult {
    pub detected_rule_confusion: bool,
    pub suggested_feedback: Option<String>,
    pub target_entity_urn: Option<EntityUrn>,
    pub persisted_entry: Option<FeedbackEntry>,
}

pub fn mine_transcript_for_rule_confusion(
    feedback_store: &EntityFeedbackStore,
    author: &IngestAuthor,
    turns: &[SessionTurn],
    rules: &[(String, String)],
) -> Result<Vec<MiningFeedbackResult>, String> {
    let indexed_rules: Vec<(&String, HashSet<String>)> = rules
        .iter()
        .map(|(rule_id, rule_body)| (rule_id, extract_rule_keywords(rule_id, rule_body)))
        .collect();

    let mut results = Vec::new();
    for turn in turns {
        if turn.role != SessionRole::Assistant && turn.role != SessionRole::User {
            continue;
        }

        let turn_tokens = tokenize(&turn.content);
        if !contains_confusion_signal(&turn_tokens) {
            continue;
        }

        for (rule_id, rule_keywords) in &indexed_rules {
            let overlap = turn_tokens.intersection(rule_keywords).count();
            if overlap < 2 {
                continue;
            }

            let urn = EntityUrn::rule(feedback_store.workspace_slug(), (*rule_id).clone())?;
            let suggested = format!(
                "Transcript mining detected likely rule confusion for {} at sequence {}",
                rule_id,
                turn.sequence
            );
            let entry = FeedbackEntry::new(
                FeedbackSource::TranscriptMined,
                urn.clone(),
                Some(FeedbackRating::Mixed),
                Some(suggested.clone()),
                Some(FeedbackNoteKind::Suggestion),
                FeedbackProvenance::new(None, author.id().map(str::to_string), None)?,
            )?;
            let persisted = feedback_store.record_entry(entry)?;
            results.push(MiningFeedbackResult {
                detected_rule_confusion: true,
                suggested_feedback: Some(suggested),
                target_entity_urn: Some(urn),
                persisted_entry: Some(persisted),
            });
        }
    }

    Ok(results)
}

fn tokenize(input: &str) -> HashSet<String> {
    input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| token.len() >= 4)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn contains_confusion_signal(tokens: &HashSet<String>) -> bool {
    const CONFUSION_MARKERS: &[&str] = &[
        "ambig",
        "conflict",
        "confus",
        "contradic",
        "error",
        "fail",
        "misap",
        "misread",
        "unclear",
        "violat",
        "wrong",
    ];

    tokens.iter().any(|token| {
        CONFUSION_MARKERS
            .iter()
            .any(|marker| token.starts_with(marker))
    })
}

fn extract_rule_keywords(
    rule_id: &str,
    rule_body: &str,
) -> HashSet<String> {
    let mut keywords = tokenize(rule_body);
    keywords.extend(tokenize(rule_id));

    const STOPWORDS: &[&str] = &[
        "about", "after", "before", "being", "must", "should", "these", "those",
        "there", "where", "which", "while", "with", "without", "would",
    ];
    for stopword in STOPWORDS {
        keywords.remove(*stopword);
    }

    keywords
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mines_transcript_with_token_overlap_and_confusion_markers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = EntityFeedbackStore::new(dir.path(), "memory-api").expect("store");
        let author = IngestAuthor::privileged_agent("ring-miner").expect("author");
        let turns = vec![SessionTurn {
            sequence: 1,
            role: SessionRole::Assistant,
            content: "The assistant misapplied instruction precedence and produced conflicting policy guidance.".into(),
            captured_at: chrono::Utc::now(),
            tool_name: None,
            model: None,
            event_meta: None,
        }];
        let rules = vec![(
            "rule-config".to_string(),
            "instruction precedence policy guidance".to_string(),
        )];

        let mined = mine_transcript_for_rule_confusion(&store, &author, &turns, &rules)
            .expect("mine");
        assert_eq!(mined.len(), 1);
        assert_eq!(mined[0].target_entity_urn.as_ref().expect("urn").entity(), "rule-config");
        assert!(mined[0].persisted_entry.is_some());
    }
}
