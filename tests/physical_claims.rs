#[path = "../examples/support/physical_claims.rs"]
mod support;
use dcra::actions::*;
use dcra::evidence::AssuranceStatus as Status;
use dcra::model_v1::*;
use support::*;

#[test]
fn short_gap_substitutes_model_claim_without_reviving_raw_evidence() {
    let mut baseline = direct(0.0);
    let mut candidate = model(0.0);
    baseline.advance_to(ms(600)).unwrap();
    candidate.advance_to(ms(600)).unwrap();
    assert!(baseline.request_lease("a", ActionPhase::Start).is_err());
    let lease = candidate.request_lease("a", ActionPhase::Start).unwrap();
    assert_eq!(
        candidate.runtime().assurance().evidence()["pressure.direct"].status,
        Status::Unknown
    );
    assert_eq!(
        candidate.certificate("pressure.model").unwrap().status,
        Status::Valid
    );
    assert!(
        candidate.runtime().leases()[&lease.lease_id]
            .evidence_versions
            .contains_key("pressure.model")
    );
    assert!(
        !candidate.runtime().leases()[&lease.lease_id]
            .evidence_versions
            .contains_key("pressure.direct")
    );
    candidate.dispatch("a", ActionPhase::Start, &lease).unwrap();
    assert!(candidate.take_command().unwrap().is_some());
}
#[test]
fn fresh_safe_case_ties_and_uncertainty_can_add_blocking() {
    assert!(direct(0.0).request_lease("a", ActionPhase::Start).is_ok());
    assert!(model(0.0).request_lease("a", ActionPhase::Start).is_ok());
    assert!(direct(0.79).request_lease("a", ActionPhase::Start).is_ok());
    let mut candidate = model(0.79);
    assert_eq!(
        candidate.certificate("pressure.model").unwrap().status,
        Status::Unknown
    );
    assert!(candidate.request_lease("a", ActionPhase::Start).is_err());
}
#[test]
fn permission_and_outcome_requirements_remain_in_place() {
    let mut candidate = model(0.0);
    candidate
        .update_permission(atom("permit", Status::Invalid, 2, ms(20000)))
        .unwrap();
    assert!(candidate.request_lease("a", ActionPhase::Start).is_err());
    candidate
        .update_permission(atom("permit", Status::Valid, 3, ms(20000)))
        .unwrap();
    let token = candidate.request_lease("a", ActionPhase::Start).unwrap();
    candidate.dispatch("a", ActionPhase::Start, &token).unwrap();
    candidate.take_command().unwrap();
    candidate.advance_to(ms(600)).unwrap();
    assert_eq!(
        candidate.runtime().instances()["a"].state,
        ActionState::Executing
    );
    assert_eq!(
        candidate
            .runtime()
            .assurance()
            .evaluation()
            .assurance_by_node["outcome"]
            .status(),
        Status::Unknown
    );
}
#[test]
fn observation_revision_revokes_a_lease_even_when_claim_remains_supported() {
    let mut candidate = model(0.0);
    let token = candidate.request_lease("a", ActionPhase::Start).unwrap();
    let old = candidate.certificate("pressure.model").unwrap().clone();
    candidate
        .observe(reading("pressure-2", 2, 0.0, ms(0)))
        .unwrap();
    let new = candidate.certificate("pressure.model").unwrap();
    assert_eq!(new.status, Status::Valid);
    assert!(new.version > old.version);
    assert!(new.state_revision > old.state_revision);
    assert!(
        candidate.runtime().leases()[&token.lease_id]
            .revoked
            .is_some()
    );
    assert!(candidate.dispatch("a", ActionPhase::Start, &token).is_err());
}
#[test]
fn time_revision_revokes_old_authority_and_renews_running_claims_without_expiry_gap() {
    let mut candidate = model(0.0);
    let token = candidate.request_lease("a", ActionPhase::Start).unwrap();
    candidate.advance_to(ms(100)).unwrap();
    assert!(candidate.dispatch("a", ActionPhase::Start, &token).is_err());
    let token = candidate.request_lease("a", ActionPhase::Start).unwrap();
    candidate.dispatch("a", ActionPhase::Start, &token).unwrap();
    candidate.take_command().unwrap();
    candidate.advance_to(ms(1500)).unwrap();
    assert_eq!(
        candidate.runtime().instances()["a"].state,
        ActionState::Executing
    );
    assert_eq!(
        candidate.certificate("pressure.model").unwrap().checked_at,
        ms(1500)
    );
}
#[test]
fn duplicate_expired_and_rejected_observations_do_not_fabricate_revisions() {
    let mut candidate = model(0.0);
    let version = candidate.certificate("pressure.model").unwrap().version;
    assert_eq!(
        candidate
            .observe(reading("pressure-1", 1, 0.0, ms(0)))
            .unwrap(),
        Update::Duplicate
    );
    assert_eq!(
        candidate.certificate("pressure.model").unwrap().version,
        version
    );
    let mut invalid = reading("bad", 2, 0.0, ms(100));
    invalid.received_at = ms(0);
    assert!(candidate.observe(invalid).is_err());
    assert_eq!(
        candidate.certificate("pressure.model").unwrap().version,
        version
    );
    candidate.advance_to(ms(600)).unwrap();
    let version = candidate.certificate("pressure.model").unwrap().version;
    assert_eq!(
        candidate
            .observe(reading("expired", 2, 0.0, ms(0)))
            .unwrap(),
        Update::Expired
    );
    assert_eq!(
        candidate.certificate("pressure.model").unwrap().version,
        version
    );
}
#[test]
fn inconsistent_knowledge_removes_claim_support_and_revokes_authority() {
    let mut candidate = model(0.0);
    let token = candidate.request_lease("a", ActionPhase::Start).unwrap();
    candidate
        .observe(reading("conflict", 2, 0.9, ms(0)))
        .unwrap();
    assert!(candidate.estimator().inconsistent());
    assert_eq!(
        candidate.certificate("pressure.model").unwrap().status,
        Status::Unknown
    );
    assert!(
        candidate.runtime().leases()[&token.lease_id]
            .revoked
            .is_some()
    );
    assert!(candidate.dispatch("a", ActionPhase::Start, &token).is_err());
}
#[test]
fn external_writes_cannot_spoof_a_model_claim() {
    let mut candidate = model(0.0);
    let before = candidate.certificate("pressure.model").unwrap().clone();
    assert!(matches!(
        candidate.update_permission(atom("pressure.model", Status::Valid, 99, ms(20000))),
        Err(IntegratedError::Runtime(ActionError::ReservedEvidence))
    ));
    assert_eq!(candidate.certificate("pressure.model").unwrap(), &before);
}
#[test]
fn migration_preserves_baseline_and_rejects_permissions_and_outcomes() {
    let (graph, definitions) = graph_and_definition();
    let config = configuration();
    let migrated = substitute_physical_claims(&graph, &definitions, &config).unwrap();
    assert_eq!(
        graph.justifications()["start.rule"].premises,
        vec!["permit", "pressure.direct"]
    );
    assert_eq!(
        migrated.justifications()["start.rule"].premises,
        vec!["permit", "pressure.model"]
    );
    assert_eq!(
        graph.justifications()["outcome.rule"],
        migrated.justifications()["outcome.rule"]
    );
    let mut bad = configuration();
    bad.replacements[0].direct_evidence = "permit".into();
    assert!(substitute_physical_claims(&graph, &definitions, &bad).is_err());
    bad.replacements[0].direct_evidence = "effect".into();
    assert!(substitute_physical_claims(&graph, &definitions, &bad).is_err());
}
#[test]
fn certificate_records_dependencies_assumptions_and_affected_phases() {
    let candidate = model(0.0);
    let c = candidate.certificate("pressure.model").unwrap();
    assert_eq!(c.schema, CLAIM_SCHEMA);
    assert_eq!(c.model_version, MODEL_VERSION);
    assert_eq!(c.configuration_revision, "claim-fixtures-v1");
    assert_eq!(c.observations[0].source, "pressure-sensor");
    assert_eq!(c.observations[0].version, 1);
    assert_eq!(c.valid_until, ms(200));
    for root in ["start", "run", "commit"] {
        assert!(c.affected_contracts.contains(&root.into()));
    }
    assert_eq!(&c.assumptions, candidate.estimator().assumptions());
}
#[test]
fn released_command_updates_certificate_dependencies() {
    let mut candidate = model(0.0);
    let token = candidate.request_lease("a", ActionPhase::Start).unwrap();
    candidate.dispatch("a", ActionPhase::Start, &token).unwrap();
    let before = candidate.certificate("pressure.model").unwrap().clone();
    let command = candidate.take_command().unwrap().unwrap();
    let after = candidate.certificate("pressure.model").unwrap();
    assert!(after.version > before.version);
    assert!(after.state_revision > before.state_revision);
    assert_eq!(after.command_ids, vec![command.model_command.id]);
}
#[test]
fn contradiction_while_running_requests_recovery_and_cannot_be_silently_cleared() {
    let mut candidate = model(0.0);
    let token = candidate.request_lease("a", ActionPhase::Start).unwrap();
    candidate.dispatch("a", ActionPhase::Start, &token).unwrap();
    candidate.take_command().unwrap();
    candidate
        .observe(reading("conflict", 2, 0.9, ms(0)))
        .unwrap();
    assert_eq!(
        candidate.runtime().instances()["a"].state,
        ActionState::Aborting
    );
    assert!(candidate.take_command().is_err());
    candidate
        .observe(reading("later-good", 3, 0.0, ms(0)))
        .unwrap();
    assert_eq!(
        candidate.certificate("pressure.model").unwrap().status,
        Status::Unknown
    );
}
#[test]
fn claim_change_revokes_a_commit_lease() {
    let mut candidate = model(0.0);
    let token = candidate.request_lease("a", ActionPhase::Start).unwrap();
    candidate.dispatch("a", ActionPhase::Start, &token).unwrap();
    let command = candidate.take_command().unwrap().unwrap();
    candidate.advance_to(ms(200)).unwrap();
    candidate
        .submit_result(ActuatorResult {
            command_id: command.runtime_command.unwrap().command_id,
            sequence: 1,
            acknowledged: Some(true),
            observed_state: std::collections::BTreeMap::new(),
            completed: false,
            observed_at: ms(200),
        })
        .unwrap();
    let token = candidate.request_lease("a", ActionPhase::Commit).unwrap();
    candidate
        .observe(reading("pressure-2", 2, 0.0, ms(200)))
        .unwrap();
    assert!(
        candidate.runtime().leases()[&token.lease_id]
            .revoked
            .is_some()
    );
    assert!(
        candidate
            .dispatch("a", ActionPhase::Commit, &token)
            .is_err()
    );
    let token = candidate.request_lease("a", ActionPhase::Commit).unwrap();
    candidate
        .dispatch("a", ActionPhase::Commit, &token)
        .unwrap();
    assert!(candidate.take_command().unwrap().is_some());
}
