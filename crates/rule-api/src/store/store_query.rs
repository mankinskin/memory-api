use super::*;

impl RuleStore {
    pub fn list(
        &self,
        filter: &RuleFilter,
        limit: Option<usize>,
    ) -> Result<Vec<RuleManifest>, RuleError> {
        let mut rules = Vec::new();

        for indexed in self.inner.list_indexed()? {
            if let Some(state) = filter.state.as_deref() {
                if indexed.state.as_deref() != Some(state) {
                    continue;
                }
            }
            if indexed.type_id != RULE_ENTRY_TYPE_ID {
                continue;
            }

            let rule = self.hydrate_rule(&indexed)?;
            if filter.matches(&rule) {
                rules.push(rule);
            }
        }

        rules.sort_by_key(|rule| {
            (
                rule.order_key().unwrap_or_default(),
                rule.slug().unwrap_or("").to_string(),
            )
        });
        if let Some(limit) = limit {
            rules.truncate(limit);
        }
        Ok(rules)
    }

    pub fn search(
        &self,
        query: &str,
        filter: &RuleFilter,
        limit: usize,
    ) -> Result<Vec<RuleManifest>, RuleError> {
        let candidates = self
            .inner
            .search(query, limit.saturating_mul(4).max(limit))?;
        let mut rules = Vec::new();

        for candidate in candidates {
            let indexed = match self.inner.get_indexed(&candidate.id)? {
                Some(indexed) => indexed,
                None => continue,
            };
            if indexed.type_id != RULE_ENTRY_TYPE_ID {
                continue;
            }

            let rule = self.hydrate_rule(&indexed)?;
            if filter.matches(&rule) {
                rules.push(rule);
            }
            if rules.len() >= limit {
                break;
            }
        }

        Ok(rules)
    }

    pub(super) fn resolve_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<Uuid>, RuleError> {
        if prefix.len() < 4 {
            return Ok(None);
        }

        let matches: Vec<_> = self
            .inner
            .list_indexed()?
            .into_iter()
            .filter(|entity| entity.id.to_string().starts_with(prefix))
            .collect();

        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some(matches[0].id)),
            _ => Err(RuleError::AmbiguousPrefix(prefix.to_string())),
        }
    }

    pub(super) fn hydrate_rule(
        &self,
        indexed: &IndexedEntity,
    ) -> Result<RuleManifest, RuleError> {
        let entity = self.read_indexed_manifest(indexed)?;
        let mut rule = entity_to_rule(&entity);
        if let Some(body) = self.read_rule_body(&indexed.path, Some(&entity)) {
            rule.set_body(&body);
        }
        Ok(rule)
    }

    pub(super) fn read_rule_body(
        &self,
        entity_path: &Path,
        entity: Option<&EntityManifest>,
    ) -> Option<String> {
        self.inner.fs.read_description(entity_path).or_else(|| {
            entity
                .and_then(|manifest| manifest.extra.get("body"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    }
}
