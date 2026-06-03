use std::process::Command;

use ticket_api::contracts::command_schema::{
    COMMAND_SCHEMA_VERSION,
    export_command_schema,
    export_command_schema_json,
};

#[test]
fn command_schema_export_is_stable() {
    let schema = export_command_schema();

    assert_eq!(schema.version, COMMAND_SCHEMA_VERSION);
    assert_eq!(schema.command_namespace, "ticket");
    assert_eq!(schema.commands.len(), 40);
    assert_eq!(schema.commands[0], "create");
    assert!(schema.commands.contains(&"batch".to_string()));
    assert!(schema.commands.contains(&"unblocked_by".to_string()));
    assert!(schema.commands.contains(&"task_create".to_string()));
    assert!(schema.commands.contains(&"task_get".to_string()));
    assert!(
        schema
            .commands
            .contains(&"task_release_promote".to_string())
    );
    assert!(schema.commands.contains(&"ready_overview".to_string()));
    assert!(schema.commands.contains(&"link".to_string()));
    assert!(schema.commands.contains(&"links".to_string()));
    assert!(schema.commands.contains(&"workspace_remove".to_string()));
    assert!(
        schema
            .commands
            .contains(&"task_assignment_start".to_string())
    );
}

#[test]
fn command_schema_json_is_machine_readable() {
    let json =
        export_command_schema_json().expect("schema export should serialize");

    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("json should parse");
    assert_eq!(parsed["version"], COMMAND_SCHEMA_VERSION);
    assert_eq!(parsed["command_namespace"], "ticket");
    assert!(parsed["commands"].is_array());
}

#[test]
fn command_schema_toon_is_machine_readable() {
    let out = Command::new(env!("CARGO_BIN_EXE_ticket"))
        .arg("--toon")
        .arg("export-command-schema")
        .output()
        .expect("ticket binary should spawn");

    assert!(
        out.status.success(),
        "ticket --toon export-command-schema failed ({})\nstdout: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let rendered =
        String::from_utf8(out.stdout).expect("toon output should be utf-8");
    let parsed: serde_json::Value = toon_format::decode_default(&rendered)
        .expect("toon output should decode");

    assert!(parsed["request_id"].is_string());
    assert_eq!(parsed["payload"]["command"], "export_command_schema");
    assert_eq!(parsed["payload"]["status"], "ok");
    assert_eq!(
        parsed["payload"]["schema"]["version"],
        COMMAND_SCHEMA_VERSION
    );
    assert_eq!(
        parsed["payload"]["schema"]["command_namespace"],
        "ticket"
    );
    assert!(parsed["payload"]["schema"]["commands"].is_array());
}
