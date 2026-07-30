use super::*;

pub(super) fn dispatch_cli(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    if operation == "move" {
        return blocked(format!(
            "cli transport for domain `{domain}` operation `move` is not wired in memory-matrix yet; in-process move cells exercise the adapter-backed move kernel"
        ));
    }

    match domain {
        "ticket" => dispatch_ticket_cli(operation, ctx),
        "spec" => dispatch_spec_cli(operation, ctx),
        "rule" => dispatch_rule_cli(operation, ctx),
        _ => blocked(format!(
            "cli transport for domain `{domain}` operation `{operation}` is not wired yet"
        )),
    }
}

fn run_ticket_cli(args: Vec<String>) -> Result<(), String> {
    let cli =
        ticket_cli::cli::parse_cli_from(args).map_err(|err| err.to_string())?;
    ticket_cli::cli::run(cli).map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch_ticket_cli(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let root = ctx.workspace_root.to_string_lossy().to_string();
    let id = uuid::Uuid::new_v4().to_string();
    let token = format!("matrix-cli-ticket-{}", uuid::Uuid::new_v4().simple());

    match operation {
        "create" => run_ticket_cli(vec![
            "ticket".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "create".into(),
            "--id".into(),
            id,
            "--type".into(),
            "tracker-improvement".into(),
            "--title".into(),
            token,
            "--state".into(),
            "open".into(),
        ]),
        "get" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id.clone(),
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token,
                "--state".into(),
                "open".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "get".into(),
                id,
            ])
        },
        "search" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id,
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token.clone(),
                "--state".into(),
                "open".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "search".into(),
                token,
                "--limit".into(),
                "10".into(),
            ])
        },
        "update" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id.clone(),
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token,
                "--state".into(),
                "open".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "update".into(),
                id,
                "--to-state".into(),
                "planned".into(),
            ])
        },
        "delete" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id.clone(),
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token,
                "--state".into(),
                "open".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "delete".into(),
                id,
            ])
        },
        "scan" => run_ticket_cli(vec![
            "ticket".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "scan".into(),
        ]),
        other => Err(format!("unknown operation `{other}`")),
    }
    .map(|_| Cell::Passed)
}

fn run_spec_cli(args: Vec<String>) -> Result<(), String> {
    let cli =
        spec_cli::cli::parse_cli_from(args).map_err(|err| err.to_string())?;
    spec_cli::cli::run(cli).map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch_spec_cli(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let root = ctx.workspace_root.to_string_lossy().to_string();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/cli/{suffix}");
    let token = format!("Matrix CLI Spec {suffix}");

    match operation {
        "create" => run_spec_cli(vec![
            "spec".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "create".into(),
            "--title".into(),
            token,
            "--slug".into(),
            slug,
            "--component".into(),
            "matrix".into(),
        ]),
        "get" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "get".into(),
                slug,
            ])
        },
        "search" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token.clone(),
                "--slug".into(),
                slug,
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "search".into(),
                token,
                "--limit".into(),
                "10".into(),
            ])
        },
        "update" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "update".into(),
                slug,
                "--field".into(),
                "scope=internal".into(),
            ])
        },
        "delete" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "delete".into(),
                slug,
            ])
        },
        "scan" => run_spec_cli(vec![
            "spec".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "scan".into(),
        ]),
        other => Err(format!("unknown operation `{other}`")),
    }
    .map(|_| Cell::Passed)
}

fn run_rule_cli(args: Vec<String>) -> Result<(), String> {
    let cli =
        rule_cli::cli::parse_cli_from(args).map_err(|err| err.to_string())?;
    rule_cli::cli::run(cli).map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch_rule_cli(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let root = ctx.workspace_root.to_string_lossy().to_string();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/cli/{suffix}");
    let token = format!("Matrix CLI Rule {suffix}");

    match operation {
        "create" => run_rule_cli(vec![
            "rule".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "create".into(),
            "--title".into(),
            token,
            "--slug".into(),
            slug,
            "--file-kind".into(),
            "markdown".into(),
            "--section".into(),
            "matrix".into(),
            "--body".into(),
            "matrix body".into(),
        ]),
        "get" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "get".into(),
                slug,
            ])
        },
        "search" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token.clone(),
                "--slug".into(),
                slug,
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "search".into(),
                token,
                "--limit".into(),
                "10".into(),
            ])
        },
        "update" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "update".into(),
                slug,
                "--body".into(),
                "updated body".into(),
            ])
        },
        "delete" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "delete".into(),
                slug,
            ])
        },
        "scan" => run_rule_cli(vec![
            "rule".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "scan".into(),
        ]),
        other => Err(format!("unknown operation `{other}`")),
    }
    .map(|_| Cell::Passed)
}
