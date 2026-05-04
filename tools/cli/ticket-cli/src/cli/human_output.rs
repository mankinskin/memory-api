use std::fmt::Write;

use serde_json::Value;

/// Render a command payload as human-readable key-value text.
///
/// Layout:
///   - Header line: `command status`
///   - Scalar fields: `key: value`
///   - Nested objects: section `[key]` with indented children
///   - Arrays of objects: section `[key] (N items)` with blank-line-separated entries
///   - Arrays of scalars: inline `key: a, b, c`
///   - Null values: shown as `-`
pub(crate) fn render_human_readable(payload: &Value) -> String {
    let Some(obj) = payload.as_object() else {
        return format_scalar(payload);
    };

    // Special case: subgraph/topgraph command renders as ASCII tree
    if matches!(obj.get("command").and_then(Value::as_str), Some("subgraph" | "topgraph")) {
        if let Some(tree) = obj.get("tree").and_then(Value::as_str) {
            return tree.to_string();
        }
    }

    // Special case: describe command prints the markdown body directly
    if obj.get("command").and_then(Value::as_str) == Some("describe") {
        return obj
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("(no description)")
            .to_string();
    }

    // Special case: health command renders findings as a report
    if obj.get("command").and_then(Value::as_str) == Some("health") {
        return render_health_report(obj);
    }

    let mut out = String::new();

    // Header: "command status"
    let command = obj.get("command").and_then(Value::as_str).unwrap_or("?");
    let status = obj.get("status").and_then(Value::as_str).unwrap_or("?");
    let _ = writeln!(out, "{command} {status}");

    let mut scalars = Vec::new();
    let mut sections = Vec::new();

    for (key, val) in obj {
        if key == "command" || key == "status" {
            continue;
        }
        if is_section(val) {
            sections.push((key.as_str(), val));
        } else {
            scalars.push((key.as_str(), val));
        }
    }

    for (key, val) in &scalars {
        let _ = writeln!(out, "{key}: {}", format_scalar(val));
    }

    for (key, val) in &sections {
        write_section(&mut out, key, val, 0);
    }

    // Trim trailing whitespace but keep one final newline
    let trimmed = out.trim_end();
    let mut result = trimmed.to_string();
    result.push('\n');
    result
}

// ── predicates ─────────────────────────────────────────────────────────────────

/// Returns true when a value should be rendered as its own `[section]`.
fn is_section(val: &Value) -> bool {
    match val {
        Value::Object(_) => true,
        Value::Array(arr) => arr.iter().any(|v| v.is_object() || v.is_array()),
        _ => false,
    }
}

// ── scalar formatting ──────────────────────────────────────────────────────────

fn format_scalar(val: &Value) -> String {
    match val {
        Value::Null => "-".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.to_string(),
        Value::Array(arr) => arr.iter().map(format_scalar).collect::<Vec<_>>().join(", "),
        // Fallback for inline objects
        Value::Object(_) => serde_json::to_string(val).unwrap_or_default(),
    }
}

// ── section rendering ──────────────────────────────────────────────────────────

fn write_section(out: &mut String, key: &str, val: &Value, depth: usize) {
    let indent = "  ".repeat(depth);

    match val {
        Value::Object(map) => {
            let _ = write!(out, "\n{indent}[{key}]\n");
            write_object_fields(out, map, depth + 1);
        }
        Value::Array(arr) => {
            let count = arr.len();
            let _ = write!(out, "\n{indent}[{key}] ({count})\n");
            write_array_items(out, arr, depth + 1);
        }
        _ => {
            let _ = writeln!(out, "{indent}{key}: {}", format_scalar(val));
        }
    }
}

fn write_object_fields(out: &mut String, map: &serde_json::Map<String, Value>, depth: usize) {
    let indent = "  ".repeat(depth);
    let mut child_sections = Vec::new();

    for (k, v) in map {
        if is_section(v) {
            child_sections.push((k.as_str(), v));
        } else {
            let _ = writeln!(out, "{indent}{k}: {}", format_scalar(v));
        }
    }

    for (k, v) in child_sections {
        write_section(out, k, v, depth);
    }
}

fn write_array_items(out: &mut String, arr: &[Value], depth: usize) {
    let indent = "  ".repeat(depth);

    for (i, item) in arr.iter().enumerate() {
        if i > 0 {
            let _ = writeln!(out);
        }
        match item {
            Value::Object(map) => {
                write_object_fields(out, map, depth);
            }
            _ => {
                let _ = writeln!(out, "{indent}{}", format_scalar(item));
            }
        }
    }
}

// ── health report ──────────────────────────────────────────────────────────────

fn render_health_report(obj: &serde_json::Map<String, Value>) -> String {
    let mut out = String::new();

    let checked = obj
        .get("tickets_checked")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let finding_count = obj
        .get("finding_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let _ = writeln!(out, "Health check: {checked} tickets checked, {finding_count} finding(s)\n");

    if let Some(findings) = obj.get("findings").and_then(Value::as_array) {
        let severity_order = |s: &str| -> u8 {
            match s {
                "error" => 0,
                "warning" => 1,
                _ => 2,
            }
        };

        let mut sorted: Vec<&Value> = findings.iter().collect();
        sorted.sort_by(|a, b| {
            let sa = a.get("severity").and_then(Value::as_str).unwrap_or("info");
            let sb = b.get("severity").and_then(Value::as_str).unwrap_or("info");
            severity_order(sa).cmp(&severity_order(sb))
        });

        for f in &sorted {
            let severity = f.get("severity").and_then(Value::as_str).unwrap_or("info");
            let check = f.get("check").and_then(Value::as_str).unwrap_or("?");
            let short_id = f.get("short_id").and_then(Value::as_str).unwrap_or("?");
            let title = f.get("title").and_then(Value::as_str).unwrap_or("?");
            let message = f.get("message").and_then(Value::as_str).unwrap_or("");

            let icon = match severity {
                "error" => "ERR ",
                "warning" => "WARN",
                _ => "INFO",
            };
            let _ = writeln!(out, "[{icon}] {short_id} {title}");
            let _ = writeln!(out, "       {check}: {message}");
        }
    }

    if finding_count == 0 {
        let _ = writeln!(out, "All checks passed.");
    } else {
        let _ = writeln!(out);
        // Summary line
        if let Some(summary) = obj.get("summary").and_then(Value::as_object) {
            let parts: Vec<String> = summary
                .iter()
                .map(|(k, v)| format!("{k}: {}", v.as_u64().unwrap_or(0)))
                .collect();
            let _ = writeln!(out, "Summary: {}", parts.join(", "));
        }
    }

    out
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flat_create_payload() {
        let payload = json!({
            "command": "create",
            "status": "ok",
            "id": "abc-123",
            "type": "tracker-improvement"
        });
        let out = render_human_readable(&payload);
        assert!(out.starts_with("create ok\n"));
        assert!(out.contains("id: abc-123"));
        assert!(out.contains("type: tracker-improvement"));
    }

    #[test]
    fn nested_object_section() {
        let payload = json!({
            "command": "get",
            "status": "ok",
            "ticket": {
                "id": "abc-123",
                "created_at": "2026-01-01"
            }
        });
        let out = render_human_readable(&payload);
        assert!(out.contains("[ticket]"));
        assert!(out.contains("  id: abc-123"));
        assert!(out.contains("  created_at: 2026-01-01"));
    }

    #[test]
    fn array_of_objects() {
        let payload = json!({
            "command": "list",
            "status": "ok",
            "count": 2,
            "items": [
                { "id": "aaa", "title": "First" },
                { "id": "bbb", "title": "Second" }
            ]
        });
        let out = render_human_readable(&payload);
        assert!(out.contains("count: 2"));
        assert!(out.contains("[items] (2)"));
        assert!(out.contains("  id: aaa"));
        assert!(out.contains("  title: First"));
        assert!(out.contains("  id: bbb"));
        assert!(out.contains("  title: Second"));
    }

    #[test]
    fn scalar_array_inline() {
        let payload = json!({
            "command": "assets",
            "status": "ok",
            "assets": ["a.png", "b.txt"]
        });
        let out = render_human_readable(&payload);
        assert!(out.contains("assets: a.png, b.txt"));
    }

    #[test]
    fn null_rendered_as_dash() {
        let payload = json!({
            "command": "get",
            "status": "ok",
            "title": null
        });
        let out = render_human_readable(&payload);
        assert!(out.contains("title: -"));
    }

    #[test]
    fn deeply_nested_status_payload() {
        let payload = json!({
            "command": "status",
            "status": "ok",
            "summary": {
                "total": 10,
                "done": 3,
                "active": 2,
                "ready": 3
            },
            "active": [
                { "id": "aaa", "title": "Bug fix", "state": "in-implementation", "component": "core" }
            ],
            "ready": [],
            "parallel_groups": []
        });
        let out = render_human_readable(&payload);
        assert!(out.contains("[summary]"));
        assert!(out.contains("  total: 10"));
        assert!(out.contains("[active] (1)"));
        assert!(out.contains("  title: Bug fix"));
        // Empty arrays render inline (no items to section)
        assert!(out.contains("ready:"));
    }

    #[test]
    fn nested_fields_in_get() {
        let payload = json!({
            "command": "get",
            "status": "ok",
            "ticket": {
                "id": "abc",
                "fields": {
                    "priority": "high",
                    "tags": ["rust", "cli"]
                }
            }
        });
        let out = render_human_readable(&payload);
        assert!(out.contains("[ticket]"));
        assert!(out.contains("  id: abc"));
        assert!(out.contains("  [fields]"));
        assert!(out.contains("    priority: high"));
        assert!(out.contains("    tags: rust, cli"));
    }

    #[test]
    fn non_object_payload_fallback() {
        let payload = json!("just a string");
        let out = render_human_readable(&payload);
        assert_eq!(out.trim(), "just a string");
    }

    #[test]
    fn dry_run_payload() {
        let payload = json!({
            "command": "create",
            "dry_run": true,
            "status": "ok",
            "would_execute": "create ticket"
        });
        let out = render_human_readable(&payload);
        assert!(out.starts_with("create ok\n"));
        assert!(out.contains("dry_run: true"));
        assert!(out.contains("would_execute: create ticket"));
    }

    #[test]
    fn audit_with_map_sections() {
        let payload = json!({
            "command": "audit",
            "status": "ok",
            "total": 10,
            "active": 8,
            "deleted": 2,
            "by_state": {
                "new": 3,
                "in-implementation": 2,
                "done": 3
            },
            "by_type": {
                "tracker-improvement": 10
            }
        });
        let out = render_human_readable(&payload);
        assert!(out.contains("total: 10"));
        assert!(out.contains("[by_state]"));
        assert!(out.contains("  new: 3"));
        assert!(out.contains("[by_type]"));
        assert!(out.contains("  tracker-improvement: 10"));
    }
}
