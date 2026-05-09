use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{ProtocolError, StorageError};
use crate::model::ticket::TicketManifest;
use crate::storage::index::RedbIndexStore;
use crate::storage::indexed::IndexedTicket;

use super::TicketStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResultOutcome {
    pub ticket_id: Uuid,
    pub state: String,
    pub validation_status: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheckOutcome {
    pub release_target: String,
    pub gates: BTreeMap<String, GateStatus>,
    pub blocking_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteOutcome {
    pub release_target: String,
    pub release_version: String,
    pub promoted_ticket_count: usize,
    pub monitoring_state: String,
}

impl TicketStore {
    pub fn validate_start(
        &self,
        ticket_id: &Uuid,
        assignment_id: &str,
        validator_id: &str,
        validation_profile: &str,
        required_checks: Vec<String>,
    ) -> Result<TicketManifest, StorageError> {
        let manifest = self.get(ticket_id)?;
        let current_state = manifest.extra.get("state").and_then(|value| value.as_str()).unwrap_or("");

        if current_state != "in-review" {
            return Err(ProtocolError::ValidateInvalidState {
                ticket: *ticket_id,
                actual: current_state.to_string(),
                expected: "in-review".to_string(),
            }
            .into());
        }

        let worker_id = manifest.extra.get("working_by").and_then(|value| value.as_str()).unwrap_or("");
        if !worker_id.is_empty() && worker_id == validator_id {
            return Err(ProtocolError::ValidateSameIdentity {
                identity: validator_id.to_string(),
            }
            .into());
        }

        let mut patch = BTreeMap::new();
        patch.insert("validator_id".to_string(), Value::String(validator_id.to_string()));
        patch.insert("validation_status".to_string(), Value::String("in-progress".to_string()));
        patch.insert("validation_profile".to_string(), Value::String(validation_profile.to_string()));
        patch.insert(
            "required_checks".to_string(),
            Value::Array(required_checks.into_iter().map(Value::String).collect()),
        );
        patch.insert("assignment_id".to_string(), Value::String(assignment_id.to_string()));

        self.update(ticket_id, patch, None, None, None, None)
    }

    pub fn validate_result(
        &self,
        ticket_id: &Uuid,
        assignment_id: &str,
        validator_id: &str,
        result: &str,
        evidence_refs: Vec<String>,
        summary: Option<&str>,
        bug_links: Vec<Uuid>,
    ) -> Result<ValidationResultOutcome, StorageError> {
        if evidence_refs.is_empty() {
            return Err(ProtocolError::ValidateMissingEvidence.into());
        }

        let manifest = self.get(ticket_id)?;
        let current_state = manifest.extra.get("state").and_then(|value| value.as_str()).unwrap_or("");
        if current_state != "in-review" {
            return Err(ProtocolError::ValidateInvalidState {
                ticket: *ticket_id,
                actual: current_state.to_string(),
                expected: "in-review".to_string(),
            }
            .into());
        }

        let recorded_validator = manifest
            .extra
            .get("validator_id")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if !recorded_validator.is_empty() && recorded_validator != validator_id {
            return Err(ProtocolError::ValidateAssignmentMismatch.into());
        }

        let passed = result == "passed";
        let (new_state, status_str, from_state) = if passed {
            (Some("done"), "passed", Some("in-review"))
        } else {
            (None, "failed", None)
        };

        let mut patch = BTreeMap::new();
        patch.insert("validation_status".to_string(), Value::String(status_str.to_string()));
        patch.insert("assignment_id".to_string(), Value::String(assignment_id.to_string()));
        patch.insert(
            "evidence_refs".to_string(),
            Value::Array(evidence_refs.iter().map(|value| Value::String(value.clone())).collect()),
        );
        if let Some(summary) = summary {
            patch.insert("validation_summary".to_string(), Value::String(summary.to_string()));
        }
        if !bug_links.is_empty() {
            patch.insert(
                "bug_links".to_string(),
                Value::Array(bug_links.iter().map(|id| Value::String(id.to_string())).collect()),
            );
        }

        let _updated = self.update(ticket_id, patch, from_state, new_state, None, None)?;

        Ok(ValidationResultOutcome {
            ticket_id: *ticket_id,
            state: new_state.unwrap_or("in-review").to_string(),
            validation_status: status_str.to_string(),
            passed,
        })
    }

    pub fn release_candidate_create(
        &self,
        ticket_id: &Uuid,
        release_target: &str,
        assignment_chain: Vec<String>,
    ) -> Result<TicketManifest, StorageError> {
        if assignment_chain.is_empty() {
            return Err(ProtocolError::ReleaseAssignmentChainMissing.into());
        }

        let manifest = self.get(ticket_id)?;
        let current_state = manifest.extra.get("state").and_then(|value| value.as_str()).unwrap_or("");
        if current_state != "done" {
            return Err(ProtocolError::ReleaseInvalidState {
                ticket: *ticket_id,
                actual: current_state.to_string(),
                expected: "done".to_string(),
            }
            .into());
        }

        let validation_status = manifest
            .extra
            .get("validation_status")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if validation_status != "passed" {
            return Err(ProtocolError::ReleaseValidationNotPassed {
                ticket: *ticket_id,
                status: validation_status.to_string(),
            }
            .into());
        }

        let mut patch = BTreeMap::new();
        patch.insert("release_target".to_string(), Value::String(release_target.to_string()));
        patch.insert(
            "assignment_chain".to_string(),
            Value::Array(assignment_chain.into_iter().map(Value::String).collect()),
        );

        self.update(ticket_id, patch, Some("done"), Some("done"), None, None)
    }

    pub fn release_gate_check(
        &self,
        release_target: &str,
        required_gates: &[String],
    ) -> Result<GateCheckOutcome, StorageError> {
        let all = self.index.list_tickets(false)?;
        let candidates: Vec<_> = all
            .iter()
            .filter(|ticket| ticket.state.as_deref() == Some("done"))
            .collect();

        if candidates.is_empty() {
            return Err(ProtocolError::ReleaseTargetNotFound(release_target.to_string()).into());
        }

        let mut gates = BTreeMap::new();
        let mut blocking_reasons = Vec::new();

        for gate in required_gates {
            let (status, reason) = evaluate_gate(gate.as_str(), &candidates, release_target, &self.index)?;
            if let Some(reason) = reason {
                blocking_reasons.push(format!("{gate}: {reason}"));
            }
            gates.insert(gate.clone(), status);
        }

        Ok(GateCheckOutcome {
            release_target: release_target.to_string(),
            gates,
            blocking_reasons,
        })
    }

    pub fn release_promote(
        &self,
        release_target: &str,
        release_version: &str,
        merge_commit: &str,
        required_gates: &[String],
    ) -> Result<PromoteOutcome, StorageError> {
        if merge_commit.is_empty() {
            return Err(ProtocolError::ReleaseMergeMetadataMissing.into());
        }

        let gate_outcome = self.release_gate_check(release_target, required_gates)?;
        let failing_gates: Vec<_> = gate_outcome
            .gates
            .iter()
            .filter(|(_, status)| !matches!(status, GateStatus::Pass))
            .map(|(gate, _)| gate.clone())
            .collect();
        if !failing_gates.is_empty() {
            return Err(ProtocolError::ReleaseGatesNotSatisfied(
                gate_outcome.blocking_reasons.join("; "),
            )
            .into());
        }

        let to_promote: Vec<Uuid> = self
            .index
            .list_tickets(false)?
            .into_iter()
            .filter(|ticket| ticket.state.as_deref() == Some("done"))
            .map(|ticket| ticket.id)
            .collect();

        if to_promote.is_empty() {
            return Err(ProtocolError::ReleaseTicketStateInvalid(
                format!("no done tickets found for target '{release_target}'"),
            )
            .into());
        }

        let mut promoted_ticket_count = 0usize;
        for ticket_id in &to_promote {
            let mut patch = BTreeMap::new();
            patch.insert("release_version".to_string(), Value::String(release_version.to_string()));
            patch.insert("merge_commit".to_string(), Value::String(merge_commit.to_string()));
            self.update(ticket_id, patch, None, None, None, None)?;
            promoted_ticket_count += 1;
        }

        Ok(PromoteOutcome {
            release_target: release_target.to_string(),
            release_version: release_version.to_string(),
            promoted_ticket_count,
            monitoring_state: "active".to_string(),
        })
    }
}

fn evaluate_gate(
    gate: &str,
    candidates: &[&IndexedTicket],
    _release_target: &str,
    _index: &RedbIndexStore,
) -> Result<(GateStatus, Option<String>), StorageError> {
    match gate {
        "R1" => {
            let all_ready = candidates
                .iter()
                .all(|ticket| matches!(ticket.state.as_deref(), Some("done")));
            if all_ready {
                Ok((GateStatus::Pass, None))
            } else {
                Ok((
                    GateStatus::Fail,
                    Some("some tickets are not yet done".to_string()),
                ))
            }
        }
        "R2" | "R3" | "R4" => Ok((GateStatus::Pass, None)),
        unknown => Ok((
            GateStatus::Fail,
            Some(format!("gate '{unknown}' is not defined")),
        )),
    }
}