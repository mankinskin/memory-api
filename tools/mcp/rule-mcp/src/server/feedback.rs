use rmcp::{
    ErrorData as McpError,
    model::CallToolResult,
};
use serde_json::json;

use rule_api::{
    FeedbackNoteKind,
    FeedbackRating,
    RuleFeedbackInput,
};

use super::{
    RecordFeedbackInput,
    RuleServer,
    query::rule_json,
};

impl RuleServer {
    pub(super) async fn rule_record_feedback_tool(
        &self,
        input: RecordFeedbackInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let rating = input
                .rating
                .parse::<FeedbackRating>()
                .map_err(|err| McpError::invalid_params(err, None))?;
            let note_kind = input
                .note_kind
                .as_deref()
                .map(str::parse::<FeedbackNoteKind>)
                .transpose()
                .map_err(|err| McpError::invalid_params(err, None))?;
            let feedback = RuleFeedbackInput::new(
                rating,
                input.note,
                note_kind,
                input.session_id,
                input.agent_or_user_id,
            )
            .map_err(|err| McpError::invalid_params(err, None))?;
            let (rule, event) = store
                .record_feedback(&input.id, feedback)
                .map_err(Self::rule_err)?;

            Self::json_result(&json!({
                "status": "ok",
                "event": event,
                "rule": rule_json(&rule),
            }))
        })
        .await
    }
}
