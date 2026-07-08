use std::{
    fs,
    path::PathBuf,
};

use tempfile::tempdir;

use super::*;
use crate::{
    manifest::RuleManifest,
    store::RuleStore,
};

fn target_node_names(target: &RenderTarget) -> Vec<String> {
    target
        .ordered_nodes()
        .into_iter()
        .map(|node| node.name)
        .collect()
}


#[path = "tests/tests_load.rs"]
mod tests_load;
#[path = "tests/tests_collect.rs"]
mod tests_collect;
#[path = "tests/tests_defaults.rs"]
mod tests_defaults;
