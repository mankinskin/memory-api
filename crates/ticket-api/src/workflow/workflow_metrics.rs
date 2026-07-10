use super::*;

pub(super) fn build_state_index(store: &TicketStore) -> HashMap<String, usize> {
    let mut state_index = HashMap::new();
    for type_id in store.schema_registry().type_ids() {
        if let Some(schema) = store.schema_registry().get(type_id) {
            for (index, state) in schema.states.iter().enumerate() {
                state_index.entry(state.clone()).or_insert(index);
            }
        }
    }
    state_index
}

pub fn parse_effort(value: &str) -> Option<u64> {
    let compact = value.trim().to_ascii_lowercase().replace([',', '_'], "");
    let chars: Vec<char> = compact.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            let start = index;
            let mut seen_dot = false;
            index += 1;
            while index < chars.len() {
                let ch = chars[index];
                if ch.is_ascii_digit() {
                    index += 1;
                    continue;
                }
                if ch == '.' && !seen_dot {
                    seen_dot = true;
                    index += 1;
                    continue;
                }
                break;
            }

            let number = compact[start..index].parse::<f64>().ok()?;
            let suffix = chars.get(index).copied();
            let multiplier = match suffix {
                Some('k') => 1_000_f64,
                Some('m') => 1_000_000_f64,
                Some('b') => 1_000_000_000_f64,
                _ => 1_f64,
            };
            return Some((number * multiplier).round() as u64);
        }
        index += 1;
    }

    None
}

pub(super) fn read_priorities(
    tickets: &[IndexedTicket]
) -> HashMap<Uuid, String> {
    tickets
        .iter()
        .filter_map(|ticket| {
            TicketFs::read(&ticket.path).ok().and_then(|manifest| {
                manifest
                    .extra
                    .get("priority")
                    .and_then(|value| value.as_str())
                    .map(|priority| (ticket.id, priority.to_string()))
            })
        })
        .collect()
}

pub(super) fn read_efforts(tickets: &[IndexedTicket]) -> HashMap<Uuid, u64> {
    tickets
        .iter()
        .filter_map(|ticket| {
            TicketFs::read(&ticket.path).ok().and_then(|manifest| {
                manifest
                    .extra
                    .get("effort")
                    .and_then(|value| value.as_str())
                    .and_then(parse_effort)
                    .map(|effort| (ticket.id, effort))
            })
        })
        .collect()
}

pub(super) fn effort_sort_key(effort: Option<u64>) -> u64 {
    effort.unwrap_or(u64::MAX)
}

pub(super) fn dependency_counts(
    tickets: &HashMap<Uuid, IndexedTicket>,
    all_edges: &[EdgeRecord],
) -> HashMap<Uuid, usize> {
    let mut counts = HashMap::new();
    for edge in all_edges {
        if edge.kind == "depends_on"
            && tickets.contains_key(&edge.from)
            && tickets.contains_key(&edge.to)
        {
            *counts.entry(edge.from).or_insert(0) += 1;
        }
    }
    counts
}

pub(super) fn dependee_counts(
    tickets: &HashMap<Uuid, IndexedTicket>,
    all_edges: &[EdgeRecord],
) -> HashMap<Uuid, usize> {
    let mut counts = HashMap::new();
    for edge in all_edges {
        if edge.kind == "depends_on"
            && tickets.contains_key(&edge.from)
            && tickets.contains_key(&edge.to)
        {
            *counts.entry(edge.to).or_insert(0) += 1;
        }
    }
    counts
}

pub(super) fn unresolved_dependency_map(
    tickets: &HashMap<Uuid, IndexedTicket>,
    all_edges: &[EdgeRecord],
) -> HashMap<Uuid, Vec<Uuid>> {
    let mut unresolved = HashMap::new();
    for edge in all_edges {
        if edge.kind != "depends_on" {
            continue;
        }
        if !tickets.contains_key(&edge.from) || !tickets.contains_key(&edge.to)
        {
            continue;
        }
        let is_resolved = tickets
            .get(&edge.to)
            .map(|ticket| is_done_state(ticket.state.as_deref()))
            .unwrap_or(false);
        if !is_resolved {
            unresolved
                .entry(edge.from)
                .or_insert_with(Vec::new)
                .push(edge.to);
        }
    }
    unresolved
}

pub(super) fn reverse_map(
    tickets: &HashMap<Uuid, IndexedTicket>,
    all_edges: &[EdgeRecord],
) -> HashMap<Uuid, Vec<Uuid>> {
    let mut reverse_map = HashMap::new();
    for edge in all_edges {
        if edge.kind == "depends_on"
            && tickets.contains_key(&edge.from)
            && tickets.contains_key(&edge.to)
        {
            reverse_map
                .entry(edge.to)
                .or_insert_with(Vec::new)
                .push(edge.from);
        }
    }
    reverse_map
}

pub(super) fn compute_metrics(
    tickets: &HashMap<Uuid, IndexedTicket>,
    state_index: &HashMap<String, usize>,
    dependency_counts: &HashMap<Uuid, usize>,
    dependee_counts: &HashMap<Uuid, usize>,
    reverse_map: &HashMap<Uuid, Vec<Uuid>>,
    workflow_facts: &HashMap<Uuid, WorkflowFacts>,
) -> HashMap<Uuid, TicketConvergenceMetrics> {
    tickets
        .keys()
        .map(|ticket_id| {
            let transitive_ids =
                reverse_dependents_for(*ticket_id, reverse_map);
            let affected_ids: Vec<Uuid> = transitive_ids
                .iter()
                .filter(|dependent_id| {
                    tickets
                        .get(dependent_id)
                        .map(|ticket| !is_done_state(ticket.state.as_deref()))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            let max_affected = affected_ids
                .iter()
                .filter_map(|dependent_id| tickets.get(dependent_id))
                .filter_map(|ticket| {
                    let state = ticket.state.clone();
                    let index = state
                        .as_deref()
                        .and_then(|value| state_index.get(value).copied());
                    index.map(|index| (state, index))
                })
                .max_by_key(|(_, index)| *index);
            let ticket_state_index = tickets
                .get(ticket_id)
                .and_then(|ticket| {
                    ticket
                        .state
                        .as_deref()
                        .and_then(|value| state_index.get(value).copied())
                })
                .unwrap_or(0);
            let (
                max_affected_dependent_state,
                max_affected_dependent_state_index,
            ) = match max_affected {
                Some((state, index)) => (state, Some(index)),
                None => (None, None),
            };
            let facts =
                workflow_facts.get(ticket_id).cloned().unwrap_or_default();

            (
                *ticket_id,
                TicketConvergenceMetrics {
                    dependency_count: dependency_counts
                        .get(ticket_id)
                        .copied()
                        .unwrap_or(0),
                    immediate_dependees: dependee_counts
                        .get(ticket_id)
                        .copied()
                        .unwrap_or(0),
                    transitive_reverse_dependents: transitive_ids.len(),
                    affected_reverse_dependent_reach: affected_ids.len(),
                    max_affected_dependent_state: max_affected_dependent_state
                        .clone(),
                    max_affected_dependent_state_index,
                    dependency_state_gap: max_affected_dependent_state_index
                        .map(|index| index.saturating_sub(ticket_state_index))
                        .unwrap_or(0),
                    became_actionable_at: facts.became_actionable_at,
                    last_blocker_progress_at: facts.last_blocker_progress_at,
                },
            )
        })
        .collect()
}

pub(super) fn reverse_dependents_for(
    root_id: Uuid,
    reverse_map: &HashMap<Uuid, Vec<Uuid>>,
) -> HashSet<Uuid> {
    let mut visited = HashSet::new();
    let mut dependents = HashSet::new();
    let mut queue = VecDeque::from([root_id]);

    while let Some(current) = queue.pop_front() {
        if !visited.insert(current) {
            continue;
        }

        for dependent in reverse_map.get(&current).into_iter().flatten() {
            if dependents.insert(*dependent) {
                queue.push_back(*dependent);
            }
        }
    }

    dependents.remove(&root_id);
    dependents
}

pub(super) fn compute_dependency_state_inversions(
    tickets: &HashMap<Uuid, IndexedTicket>,
    all_edges: &[EdgeRecord],
    state_index: &HashMap<String, usize>,
    metrics: &HashMap<Uuid, TicketConvergenceMetrics>,
) -> HashMap<Uuid, Vec<DependencyStateInversion>> {
    let mut inversions = HashMap::<Uuid, Vec<DependencyStateInversion>>::new();

    for edge in all_edges {
        if edge.kind != "depends_on" {
            continue;
        }
        let Some(dependent) = tickets.get(&edge.from) else {
            continue;
        };
        let Some(prerequisite) = tickets.get(&edge.to) else {
            continue;
        };
        if is_done_state(dependent.state.as_deref())
            || is_done_state(prerequisite.state.as_deref())
        {
            continue;
        }

        let dependent_index = dependent
            .state
            .as_deref()
            .and_then(|state| state_index.get(state).copied())
            .unwrap_or(0);
        let prerequisite_index = prerequisite
            .state
            .as_deref()
            .and_then(|state| state_index.get(state).copied())
            .unwrap_or(0);
        if dependent_index <= prerequisite_index {
            continue;
        }

        let prerequisite_metrics =
            metrics.get(&prerequisite.id).cloned().unwrap_or_default();
        inversions
            .entry(dependent.id)
            .or_insert_with(Vec::new)
            .push(DependencyStateInversion {
                dependent_id: dependent.id,
                dependent_title: dependent.title.clone(),
                dependent_state: dependent.state.clone(),
                prerequisite_id: prerequisite.id,
                prerequisite_title: prerequisite.title.clone(),
                prerequisite_state: prerequisite.state.clone(),
                dependency_state_gap: dependent_index
                    .saturating_sub(prerequisite_index),
                affected_reverse_dependent_reach: prerequisite_metrics
                    .affected_reverse_dependent_reach,
                transitive_reverse_dependents: prerequisite_metrics
                    .transitive_reverse_dependents,
            });
    }

    for issues in inversions.values_mut() {
        issues.sort_by(|left, right| {
            right
                .dependency_state_gap
                .cmp(&left.dependency_state_gap)
                .then_with(|| {
                    right
                        .affected_reverse_dependent_reach
                        .cmp(&left.affected_reverse_dependent_reach)
                })
                .then_with(|| left.prerequisite_id.cmp(&right.prerequisite_id))
        });
    }

    inversions
}

pub(super) fn is_done_state(state: Option<&str>) -> bool {
    matches!(state, Some("done" | "cancelled"))
}

pub(super) fn is_candidate_state(state: Option<&str>) -> bool {
    state
        .map(|value| {
            !DONE_STATES.contains(&value) && !PAUSED_STATES.contains(&value)
        })
        .unwrap_or(true)
}

pub(super) fn ticket_title(ticket: &IndexedTicket) -> &str {
    ticket.title.as_deref().unwrap_or("")
}

pub(super) fn priority_weight(priority: &str) -> u8 {
    match priority {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        "backlog" => 5,
        _ => 4,
    }
}
