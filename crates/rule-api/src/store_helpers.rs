use super::*;

pub(super) fn validate_slug(slug: &str) -> Result<(), RuleError> {
    if slug.is_empty() {
        return Err(RuleError::InvalidSlug("slug cannot be empty".to_string()));
    }

    let valid = slug.chars().all(|ch| {
        ch.is_ascii_lowercase()
            || ch.is_ascii_digit()
            || matches!(ch, '/' | '-' | '_' | '.')
    });

    if valid {
        Ok(())
    } else {
        Err(RuleError::InvalidSlug(slug.to_string()))
    }
}

pub(super) fn rule_to_entity(rule: &RuleManifest) -> EntityManifest {
    let mut extra = rule.extra.clone();
    extra.remove("body");

    EntityManifest {
        id: rule.id,
        created_at: rule.created_at,
        extra,
    }
}

pub(super) fn entity_to_rule(entity: &EntityManifest) -> RuleManifest {
    RuleManifest {
        id: entity.id,
        created_at: entity.created_at,
        extra: entity.extra.clone(),
    }
}

pub(super) fn is_missing_index_entry(indexed: &IndexedEntity) -> bool {
    !indexed.path.is_dir() || !indexed.path.join(RULE_MANIFEST_FILE).is_file()
}

pub(super) fn feedback_events_path(
    fs: &EntityFs,
    entity_path: &Path,
) -> PathBuf {
    entity_path
        .join(fs.config.assets_dir)
        .join(FEEDBACK_DIR)
        .join(FEEDBACK_EVENTS_FILE)
}

pub(super) fn append_feedback_event(
    fs: &EntityFs,
    entity_path: &Path,
    event: &RuleFeedbackEvent,
) -> Result<(), RuleError> {
    fs.ensure_assets_dir(entity_path)?;
    let path = feedback_events_path(fs, entity_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(StorageError::Io)?;
    }

    let line = serde_json::to_string(event)
        .map_err(|err| StorageError::Serialization(err.to_string()))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(StorageError::Io)?;
    writeln!(file, "{line}").map_err(StorageError::Io)?;
    Ok(())
}

pub(super) fn read_feedback_events(
    fs: &EntityFs,
    entity_path: &Path,
) -> Result<Vec<RuleFeedbackEvent>, RuleError> {
    let path = feedback_events_path(fs, entity_path);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&path).map_err(StorageError::Io)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(StorageError::Io)?;
        if line.trim().is_empty() {
            continue;
        }
        let event = serde_json::from_str(&line).map_err(|err| {
            RuleError::Asset(format!(
                "invalid feedback event at {}:{}: {err}",
                path.display(),
                index + 1,
            ))
        })?;
        events.push(event);
    }

    Ok(events)
}

pub(super) fn feedback_summary_patch(
    summary: &FeedbackSummary
) -> BTreeMap<String, Value> {
    let mut patch = BTreeMap::from([
        (
            "feedback_helpful_count".to_string(),
            Value::Number(Number::from(summary.helpful_count)),
        ),
        (
            "feedback_mixed_count".to_string(),
            Value::Number(Number::from(summary.mixed_count)),
        ),
        (
            "feedback_not_helpful_count".to_string(),
            Value::Number(Number::from(summary.not_helpful_count)),
        ),
        (
            "feedback_note_count".to_string(),
            Value::Number(Number::from(summary.note_count)),
        ),
        (
            "feedback_unresolved_count".to_string(),
            Value::Number(Number::from(summary.unresolved_count)),
        ),
    ]);

    if let Some(last_at) = &summary.last_at {
        patch.insert(
            "feedback_last_at".to_string(),
            Value::String(last_at.clone()),
        );
    }

    patch
}
