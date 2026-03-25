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
    assert_eq!(schema.commands.len(), 39);
    assert_eq!(schema.commands[0], "create");
    assert!(schema.commands.contains(&"batch".to_string()));
    assert!(schema.commands.contains(&"task_create".to_string()));
    assert!(schema.commands.contains(&"task_get".to_string()));
    assert!(schema.commands.contains(&"task_release_promote".to_string()));
    assert!(schema.commands.contains(&"ready_overview".to_string()));
    assert!(schema.commands.contains(&"link".to_string()));
    assert!(schema.commands.contains(&"links".to_string()));
    assert!(schema.commands.contains(&"workspace_remove".to_string()));
    assert!(schema.commands.contains(&"task_assignment_start".to_string()));
}

#[test]
fn command_schema_json_is_machine_readable() {
    let json = export_command_schema_json().expect("schema export should serialize");

    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json should parse");
    assert_eq!(parsed["version"], COMMAND_SCHEMA_VERSION);
    assert_eq!(parsed["command_namespace"], "ticket");
    assert!(parsed["commands"].is_array());
}
