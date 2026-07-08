use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_stdio_read_error_prefers_non_zero_exit() {
        assert_eq!(
            classify_stdio_read_error(
                Some(2),
                std::io::ErrorKind::UnexpectedEof,
            ),
            "non_zero_exit"
        );
    }

    #[test]
    fn classify_stdio_read_error_maps_unexpected_eof_without_status() {
        assert_eq!(
            classify_stdio_read_error(None, std::io::ErrorKind::UnexpectedEof),
            "unexpected_eof"
        );
    }

    #[test]
    fn classify_stdio_read_error_maps_other_io_failures() {
        assert_eq!(
            classify_stdio_read_error(None, std::io::ErrorKind::BrokenPipe),
            "io_read_failure"
        );
    }

    #[test]
    fn validate_sentinel_ticket_id_detects_mismatch() {
        let get_json = serde_json::json!({
            "ticket": {
                "id": "returned-2"
            }
        });
        let err = validate_sentinel_ticket_id("created-1", &get_json)
            .expect_err("mismatched ids should fail");
        assert!(err.contains("mismatched id"));
    }

    #[test]
    fn extract_stdio_tool_json_reports_parse_decode_failure() {
        let payload = serde_json::json!({
            "content": [{
                "type": "text",
                "text": "{not valid json"
            }]
        });
        let err = extract_stdio_tool_json(&payload)
            .expect_err("invalid text payload should fail json decoding");
        assert!(err.contains("parse mcp tools/call text payload"));
    }

    #[test]
    fn build_failure_bundle_filters_non_whitelisted_env_selectors() {
        let mut env_selectors = serde_json::Map::new();
        env_selectors.insert(
            "TICKET_INDEX_ROOT".to_string(),
            serde_json::Value::String("C:/tmp/tickets".to_string()),
        );
        env_selectors.insert(
            "UNSAFE_SECRET_KEY".to_string(),
            serde_json::Value::String("top-secret".to_string()),
        );

        let bundle = build_failure_bundle(
            "spawn_failure",
            "cargo",
            &["run".to_string()],
            &PathBuf::from("C:/tmp"),
            &env_selectors,
            "spawn_ticket_mcp",
            None,
            "",
            "",
            "spawn failed",
        );
        let json: serde_json::Value =
            serde_json::from_str(&bundle).expect("bundle should be valid json");
        let selectors = json["invocation"]["env_selectors"]
            .as_object()
            .expect("env_selectors should be object");
        assert!(selectors.contains_key("TICKET_INDEX_ROOT"));
        assert!(!selectors.contains_key("UNSAFE_SECRET_KEY"));
    }

    #[test]
    fn tail_from_bytes_returns_bounded_suffix() {
        let content = "A".repeat(STDIO_TAIL_BYTES + 64);
        let tail = tail_from_bytes(content.as_bytes());
        assert_eq!(tail.len(), STDIO_TAIL_BYTES);
        assert!(tail.chars().all(|ch| ch == 'A'));
    }
}
