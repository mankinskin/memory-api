use std::{
    collections::{
        HashMap,
        HashSet,
    },
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

use crate::{
    error::RuleError,
    manifest::{
        RuleId,
        RuleManifest,
    },
    store::{
        RuleFilter,
        RuleStore,
    },
};

#[cfg(test)]
mod tests;

#[path = "targets_loader.rs"]
mod targets_loader;
use targets_loader::*;

#[path = "targets_model.rs"]
mod targets_model;
use targets_model::*;
pub use targets_model::{
    ExplainedRuleMatch,
    ExplainedTarget,
    ExplainedTargetNode,
    RenderTarget,
    RenderTargetConfig,
    RenderTargetFilter,
    RenderTargetNode,
};

pub fn collect_target_rules(
    store: &RuleStore,
    target: &RenderTarget,
) -> Result<Vec<RuleManifest>, RuleError> {
    let inherited = target.flat_filter();
    let mut collected = Vec::new();
    let mut seen = HashSet::<RuleId>::new();

    for node in target.ordered_nodes() {
        collect_target_node_rules(
            store,
            target,
            &node,
            &inherited,
            &mut seen,
            &mut collected,
        )?;
    }

    Ok(collected)
}

pub fn explain_target(
    store: &RuleStore,
    target: &RenderTarget,
) -> Result<ExplainedTarget, RuleError> {
    let root_filter = target.flat_filter();
    let mut matched_rule_count = 0usize;
    let mut seen = HashSet::<RuleId>::new();
    let mut nodes = Vec::new();

    for node in target.ordered_nodes() {
        nodes.push(explain_target_node(
            store,
            target,
            &node,
            &root_filter,
            &mut seen,
            &mut matched_rule_count,
        )?);
    }

    Ok(ExplainedTarget {
        name: target.name.clone(),
        output_path: target.output_path.clone(),
        root_filter,
        matched_rule_count,
        nodes,
    })
}

#[path = "targets_query.rs"]
mod targets_query;
pub(crate) use targets_query::directory_target_config_error;
use targets_query::*;
pub use targets_query::{
    TargetConfigError,
    load_render_target_config,
    render_target_by_name,
    render_target_by_selector,
    resolve_render_target_output,
};
