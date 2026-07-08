use chrono::Utc;

use crate::matrix::{
    CellResult,
    DomainOps,
    MatrixCtx,
    pass,
};

pub(crate) struct SessionDomain;

impl SessionDomain {
    fn config(ctx: &MatrixCtx) -> session_api::SessionStoreConfig {
        session_api::SessionStoreConfig::new(
            ctx.store_root(".session-matrix"),
            "default",
        )
    }

    fn payload(
        session_id: &str,
        content: &str,
    ) -> session_api::CopilotHookPayload {
        Self::payload_multi(session_id, &[content])
    }

    fn payload_multi(
        session_id: &str,
        contents: &[&str],
    ) -> session_api::CopilotHookPayload {
        session_api::CopilotHookPayload {
            session_id: session_id.to_string(),
            workspace_slug: "default".to_string(),
            captured_at: Utc::now(),
            conversation_id: None,
            agent_id: Some("matrix".to_string()),
            model: None,
            trigger: Some("matrix".to_string()),
            messages: contents
                .iter()
                .map(|content| session_api::CopilotHookMessage {
                    role: session_api::SessionRole::User,
                    content: (*content).to_string(),
                    tool_name: None,
                    captured_at: None,
                    event_meta: None,
                })
                .collect(),
            events: vec![],
            runtime: None,
        }
    }
}

impl DomainOps for SessionDomain {
    fn domain(&self) -> &'static str {
        "session"
    }

    fn create(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let config = Self::config(ctx);
        config
            .capture_copilot_hook(Self::payload("matrix-create", "hello"))
            .map_err(|err| err.to_string())?;
        config
            .read_session("matrix-create")
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let config = Self::config(ctx);
        if config.read_session("fixture-session").is_err() {
            config
                .capture_copilot_hook(Self::payload(
                    "fixture-session",
                    "seeded turn",
                ))
                .map_err(|err| err.to_string())?;
        }
        let record = config
            .read_session("fixture-session")
            .map_err(|err| err.to_string())?;
        if record.session_id == "fixture-session" && record.has_turns() {
            pass()
        } else {
            Err(format!("unexpected session id: {}", record.session_id))
        }
    }

    fn search(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let config = Self::config(ctx);
        if config.read_session("fixture-session").is_err() {
            config
                .capture_copilot_hook(Self::payload(
                    "fixture-session",
                    "seeded turn",
                ))
                .map_err(|err| err.to_string())?;
        }
        let records = config
            .query_sessions(&session_api::SessionQuery {
                session_id_prefix: Some("fixture-session".to_string()),
                ..Default::default()
            })
            .map_err(|err| err.to_string())?;
        if records.is_empty() {
            return Err("session query returned no records".to_string());
        }
        pass()
    }

    fn update(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let config = Self::config(ctx);
        config
            .capture_copilot_hook(Self::payload("matrix-update", "first"))
            .map_err(|err| err.to_string())?;
        let before = config
            .read_session("matrix-update")
            .map_err(|err| err.to_string())?;
        config
            .capture_copilot_hook(Self::payload_multi(
                "matrix-update",
                &["first", "second"],
            ))
            .map_err(|err| err.to_string())?;
        let after = config
            .read_session("matrix-update")
            .map_err(|err| err.to_string())?;
        if after.turns.len() > before.turns.len() {
            pass()
        } else {
            Err("append capture did not grow the session transcript"
                .to_string())
        }
    }
}
