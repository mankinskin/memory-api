pub trait SessionPinFeedbackSink {
    fn record_pin_usage(
        &self,
        workspace_session_id: &str,
        run_id: &str,
        entity_urn: &str,
    ) -> Result<(), String>;
}
