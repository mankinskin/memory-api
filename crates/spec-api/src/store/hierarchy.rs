use std::collections::VecDeque;

use crate::{
    error::SpecError,
    manifest::SpecManifest,
};

use super::{
    SpecStore,
    read_spec_manifest,
};

impl SpecStore {
    pub fn children(
        &self,
        id_or_slug: &str,
    ) -> Result<Vec<SpecManifest>, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let uuid_str = uuid.to_string();
        let mut children = Vec::new();

        for indexed in &self.inner.list_indexed()? {
            if let Ok(spec) = read_spec_manifest(&indexed.path) {
                if spec.parent() == Some(&uuid_str) {
                    children.push(spec);
                }
            }
        }

        Ok(children)
    }

    pub fn ancestors(
        &self,
        id_or_slug: &str,
    ) -> Result<Vec<SpecManifest>, SpecError> {
        let mut result = Vec::new();
        let mut current = self.get(id_or_slug)?;

        while let Some(parent_str) = current.parent().map(String::from) {
            let parent = self.get(&parent_str)?;
            result.push(parent.clone());
            current = parent;
        }

        Ok(result)
    }

    pub fn subtree(
        &self,
        id_or_slug: &str,
    ) -> Result<Vec<SpecManifest>, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let all = self.inner.list_indexed()?;
        let mut result = Vec::new();
        let mut queue = VecDeque::from([uuid]);

        while let Some(current_id) = queue.pop_front() {
            let current_str = current_id.to_string();
            for indexed in &all {
                if let Ok(spec) = read_spec_manifest(&indexed.path) {
                    if spec.parent() == Some(&current_str) {
                        queue.push_back(spec.id);
                        result.push(spec);
                    }
                }
            }
        }

        Ok(result)
    }
}
