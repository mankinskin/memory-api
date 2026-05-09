use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use rmcp::{
    ErrorData as McpError,
    model::CallToolResult,
};
use serde_json::json;

use rule_api::{
    RenderTarget,
    RuleFilter,
    collect_target_rules,
    explain_target,
    load_render_target_config,
    render_markdown_file,
    render_target_by_name,
    resolve_render_target_output,
};

use super::{
    ExplainRuleTargetInput,
    GenerateRuleFileInput,
    GenerateRuleTargetInput,
    RuleServer,
};

impl RuleServer {
    pub(super) async fn rule_generate_file_tool(
        &self,
        input: GenerateRuleFileInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            validate_generate_input(&input)?;
            let filter = RuleFilter {
                state: input.state.clone(),
                file_kind: Some(input.file_kind.clone()),
                section: input.section.clone(),
                repo_scope: Some(input.repo_scope.clone()),
                path_scope: input.path_scope.clone(),
                slug: None,
                has_unresolved_feedback: None,
            };
            let rules = store.list(&filter, None).map_err(Self::rule_err)?;
            let rendered = render_markdown_file(&rules);

            if input.check {
                let output = input
                    .output_path
                    .as_deref()
                    .expect("validated output path");
                ensure_generated_output_matches(output, &rendered)?;
            } else if !input.dry_run {
                let output = input
                    .output_path
                    .as_deref()
                    .expect("validated output path");
                write_generated_output(output, &rendered)?;
            }

            Self::json_result(&json!({
                "status": "ok",
                "count": rules.len(),
                "file_kind": input.file_kind,
                "repo_scope": input.repo_scope,
                "path_scope": input.path_scope,
                "section": input.section,
                "output_path": input.output_path,
                "dry_run": input.dry_run,
                "check": input.check,
                "content": input.dry_run.then_some(rendered),
            }))
        })
        .await
    }

    pub(super) async fn rule_generate_target_tool(
        &self,
        input: GenerateRuleTargetInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            validate_generate_target_input(&input)?;
            let config_path = PathBuf::from(&input.config_path);
            let config = load_render_target_config(&config_path)
                .map_err(Self::target_config_err)?;
            let target = render_target_by_name(&config, &input.target)
                .map_err(Self::target_config_err)?;
            let output = resolve_render_target_output(&config_path, target);
            let payload = generate_target_payload(
                store,
                target,
                input.dry_run,
                input.check,
                &output,
            )?;

            Self::json_result(&json!({
                "status": "ok",
                "target": input.target,
                "output_path": output,
                "count": payload.count,
                "file_kind": target.file_kind,
                "repo_scope": target.repo_scope,
                "path_scope": target.path_scope,
                "section": target.section,
                "dry_run": input.dry_run,
                "check": input.check,
                "content": payload.content,
            }))
        })
        .await
    }

    pub(super) async fn rule_explain_target_tool(
        &self,
        input: ExplainRuleTargetInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let config_path = PathBuf::from(&input.config_path);
            let config = load_render_target_config(&config_path)
                .map_err(Self::target_config_err)?;
            let target = render_target_by_name(&config, &input.target)
                .map_err(Self::target_config_err)?;
            let output = resolve_render_target_output(&config_path, target);
            let outline =
                explain_target(store, target).map_err(Self::rule_err)?;

            Self::json_result(&json!({
                "status": "ok",
                "target": input.target,
                "output_path": output,
                "outline": outline,
            }))
        })
        .await
    }
}

fn validate_generate_input(
    input: &GenerateRuleFileInput
) -> Result<(), McpError> {
    if input.check && input.dry_run {
        return Err(McpError::invalid_params(
            "choose either check or dry_run".to_string(),
            None,
        ));
    }

    if (input.check || !input.dry_run) && input.output_path.is_none() {
        return Err(McpError::invalid_params(
            "output_path is required unless dry_run is true".to_string(),
            None,
        ));
    }

    Ok(())
}

fn validate_generate_target_input(
    input: &GenerateRuleTargetInput
) -> Result<(), McpError> {
    if input.check && input.dry_run {
        return Err(McpError::invalid_params(
            "choose either check or dry_run".to_string(),
            None,
        ));
    }

    Ok(())
}

fn ensure_generated_output_matches(
    output: &str,
    rendered: &str,
) -> Result<(), McpError> {
    let path = PathBuf::from(output);
    let existing = fs::read_to_string(&path).map_err(|err| {
        McpError::invalid_params(
            format!("read generated file {}: {err}", path.display()),
            None,
        )
    })?;

    if existing == rendered {
        Ok(())
    } else {
        Err(McpError::invalid_params(
            format!("generated output differs from {}", path.display()),
            None,
        ))
    }
}

fn write_generated_output(
    output: &str,
    rendered: &str,
) -> Result<(), McpError> {
    let path = PathBuf::from(output);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            McpError::internal_error(
                format!("create {}: {err}", parent.display()),
                None,
            )
        })?;
    }

    fs::write(&path, rendered).map_err(|err| {
        McpError::internal_error(
            format!("write generated file {}: {err}", path.display()),
            None,
        )
    })
}

struct GenerateTargetPayload {
    count: usize,
    content: Option<String>,
}

fn generate_target_payload(
    store: &rule_api::RuleStore,
    target: &RenderTarget,
    dry_run: bool,
    check: bool,
    output: &Path,
) -> Result<GenerateTargetPayload, McpError> {
    let rules =
        collect_target_rules(store, target).map_err(RuleServer::rule_err)?;
    let rendered = render_markdown_file(&rules);

    if check {
        ensure_generated_output_matches(
            output.to_string_lossy().as_ref(),
            &rendered,
        )?;
    } else if !dry_run {
        write_generated_output(output.to_string_lossy().as_ref(), &rendered)?;
    }

    Ok(GenerateTargetPayload {
        count: rules.len(),
        content: dry_run.then_some(rendered),
    })
}
