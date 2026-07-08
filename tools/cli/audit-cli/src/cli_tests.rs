use super::*;
use tempfile::tempdir;

#[test]
fn parses_move_command() {
    let cli = parse_cli_from([
        "audit",
        "move",
        "7b3a7c62-1f3f-45d6-b8a1-f2b83e3d9f71",
        "--repo-root",
        "/repo",
        "--to-workspace-root",
        "/target",
    ])
    .expect("parse move");

    match cli.command {
        AuditCommand::Move(args) => {
            assert_eq!(args.id.as_deref(), Some("7b3a7c62-1f3f-45d6-b8a1-f2b83e3d9f71"));
            assert_eq!(args.repo_root, PathBuf::from("/repo"));
            assert_eq!(args.to_workspace_root, Some(PathBuf::from("/target")));
        },
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn move_plans_blocked_when_audit_entity_has_no_folder() {
    let temp = tempdir().unwrap();
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["init"])
        .status()
        .expect("git init")
        .success()
        .then_some(())
        .expect("git init failed");

    let target_workspace = repo_root.join("target-workspace");
    std::fs::create_dir_all(target_workspace.join(".audit")).unwrap();

    let cli = parse_cli_from([
        "audit",
        "--json",
        "move",
        "7b3a7c62-1f3f-45d6-b8a1-f2b83e3d9f71",
        "--repo-root",
        repo_root.to_string_lossy().as_ref(),
        "--to-workspace-root",
        target_workspace.to_string_lossy().as_ref(),
    ])
    .expect("parse move");

    match run(cli).expect("run move") {
        CliOutput::Machine(value, _) => {
            assert_eq!(value["status"], "blocked");
            assert_eq!(value["mode"], "plan");
            assert!(value["plan"]["blockers"].as_array().unwrap().len() > 0);
        },
        other => panic!("unexpected output: {other:?}"),
    }
}
