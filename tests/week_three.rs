#[path = "../examples/support/actions.rs"]
mod fixture;
use dcra::actions::*;
use dcra::evidence::{AssuranceStatus as Status, AssuranceValue as Value};
use dcra::runtime::EvaluationMode;
use fixture::*;
use std::collections::{BTreeMap, BTreeSet};
fn normal(mode: EvaluationMode) -> ActionRuntime {
    runtime(
        mode,
        Interruptibility::Preemptible,
        RecoveryType::Reversible,
    )
}
fn modes() -> [EvaluationMode; 2] {
    [EvaluationMode::Full, EvaluationMode::Incremental]
}

#[test]
fn c5_stale_lease_requires_fresh_authority_and_unrelated_epoch_is_allowed() {
    for mode in modes() {
        let mut r = normal(mode);
        let old = r.request_lease("a", ActionPhase::Start).unwrap();
        r.update(observation("start", Status::Valid, 2, 100))
            .unwrap();
        assert_eq!(
            r.dispatch("a", ActionPhase::Start, &old),
            Err(ActionError::RevokedLease(LeaseRevocation::EvidenceChanged))
        );
        assert!(r.commands().is_empty());
        let fresh = r.request_lease("a", ActionPhase::Start).unwrap();
        r.update(observation("other", Status::Unknown, 2, 100))
            .unwrap();
        r.dispatch("a", ActionPhase::Start, &fresh).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        assert_eq!(r.commands().len(), 1);
        assert_eq!(
            r.dispatch("a", ActionPhase::Start, &fresh),
            Err(ActionError::ConsumedLease)
        );
        assert_eq!(r.commands().len(), 1);
    }
}
#[test]
fn c6_old_plan_cancels_unstarted_actions_and_blocks_old_authority() {
    for mode in modes() {
        let mut r = normal(mode);
        let token = r.request_lease("a", ActionPhase::Start).unwrap();
        r.change_plan_version("plan", 8).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Cancelled);
        assert_eq!(r.instances()["a"].plan_version, 7);
        assert_eq!(
            r.dispatch("a", ActionPhase::Start, &token),
            Err(ActionError::RevokedLease(LeaseRevocation::PlanChanged))
        );
        assert!(r.commands().is_empty());
        assert_eq!(
            r.change_plan_version("plan", 8),
            Err(ActionError::PlanVersion)
        );
    }
}
#[test]
fn c7_running_expiry_aborts_at_intermediate_deadline_and_old_command_is_superseded() {
    for mode in modes() {
        let mut r = normal(mode);
        r.update(observation("run_a", Status::Valid, 2, 2)).unwrap();
        let start = start(&mut r);
        r.take_command().unwrap();
        r.advance_to(secs(5)).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Aborting);
        let abort = r.take_command().unwrap();
        assert_eq!(abort.kind, CommandKind::SafeAbort);
        assert_eq!(abort.issued_at, secs(2));
        assert_eq!(
            r.submit_result(report(
                &start,
                1,
                r.now(),
                Some(true),
                true,
                Some(Status::Valid)
            )),
            Err(ActionError::SupersededCommand)
        );
        r.submit_result(report(
            &abort.command_id,
            1,
            r.now(),
            Some(true),
            true,
            None,
        ))
        .unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Cancelled);
        assert!(r.running_by_root().is_empty());
    }
}
#[test]
fn c8_start_loss_does_not_abort_and_alternative_running_support_refreshes_subscriptions() {
    for mode in modes() {
        let mut r = normal(mode);
        start(&mut r);
        r.update(observation("start", Status::Unknown, 2, 100))
            .unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        r.update(observation("run_b", Status::Valid, 1, 100))
            .unwrap();
        r.update(observation("run_a", Status::Unknown, 2, 100))
            .unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        assert_eq!(r.commands().len(), 1);
        assert!(r.running_by_evidence().contains_key("run_b"));
        assert!(!r.running_by_evidence().contains_key("run_a"));
    }
}
#[test]
fn c9_irreversible_commit_needs_fresh_valid_commit_evidence() {
    for mode in modes() {
        let mut r = runtime(
            mode,
            Interruptibility::Irreversible,
            RecoveryType::Irreversible,
        );
        start(&mut r);
        let preparation = r.take_command().unwrap();
        r.submit_result(report(
            &preparation.command_id,
            1,
            r.now(),
            Some(true),
            false,
            None,
        ))
        .unwrap();
        let commit = r.request_lease("a", ActionPhase::Commit).unwrap();
        r.update(observation("commit", Status::Unknown, 2, 100))
            .unwrap();
        assert!(r.dispatch("a", ActionPhase::Commit, &commit).is_err());
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        assert_eq!(r.commands().len(), 1);
        assert_eq!(
            r.request_lease("a", ActionPhase::Commit),
            Err(ActionError::ContractUnavailable(Status::Unknown))
        );
        r.update(observation("commit", Status::Valid, 3, 100))
            .unwrap();
        let token = r.request_lease("a", ActionPhase::Commit).unwrap();
        r.dispatch("a", ActionPhase::Commit, &token).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Committed);
        assert_eq!(
            r.dispatch("a", ActionPhase::Commit, &token),
            Err(ActionError::ConsumedLease)
        );
    }
}
#[test]
fn c10_missing_acknowledgement_cannot_complete_or_unlock_successor() {
    for mode in modes() {
        let mut r = normal(mode);
        let command = start(&mut r);
        r.take_command().unwrap();
        r.advance_to(secs(2)).unwrap();
        r.submit_result(report(
            &command,
            1,
            r.now(),
            None,
            true,
            Some(Status::Valid),
        ))
        .unwrap();
        assert_eq!(
            r.assurance().evaluation().assurance_by_node["a.outcome"],
            Value::Unknown
        );
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        assert_eq!(
            r.request_lease("b", ActionPhase::Start),
            Err(ActionError::DependenciesIncomplete)
        );
        r.submit_result(report(
            &command,
            2,
            r.now(),
            Some(true),
            false,
            Some(Status::Valid),
        ))
        .unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        r.submit_result(report(
            &command,
            3,
            r.now(),
            Some(true),
            true,
            Some(Status::Valid),
        ))
        .unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Completed);
        assert_eq!(r.instances()["b"].state, ActionState::Ready);
    }
}
#[test]
fn c17_unknown_requests_evidence_invalid_requests_replanning() {
    for mode in modes() {
        let mut r = normal(mode);
        for (version, status, decision) in [
            (2, Status::Unknown, DecisionKind::Revalidate),
            (3, Status::Invalid, DecisionKind::BlockAndReplan),
        ] {
            r.update(observation("start", status, version, 100))
                .unwrap();
            assert_eq!(
                r.request_lease("a", ActionPhase::Start),
                Err(ActionError::ContractUnavailable(status))
            );
            assert_eq!(r.decisions().last().unwrap().kind, decision);
        }
        assert!(r.commands().is_empty());
    }
}
#[test]
fn lease_deadline_wrong_nonce_phase_and_instance_never_dispatch() {
    for mode in modes() {
        let mut r = normal(mode);
        let token = r.request_lease("a", ActionPhase::Start).unwrap();
        let wrong = LeaseToken {
            lease_id: token.lease_id.clone(),
            nonce: "wrong".into(),
        };
        assert_eq!(
            r.dispatch("a", ActionPhase::Start, &wrong),
            Err(ActionError::WrongLease)
        );
        assert_eq!(
            r.dispatch("b", ActionPhase::Start, &token),
            Err(ActionError::WrongLease)
        );
        assert_eq!(
            r.dispatch("a", ActionPhase::Commit, &token),
            Err(ActionError::WrongLease)
        );
        r.advance_to(secs(3)).unwrap();
        assert_eq!(
            r.dispatch("a", ActionPhase::Start, &token),
            Err(ActionError::RevokedLease(LeaseRevocation::Expired))
        );
        assert!(r.commands().is_empty());
    }
}
#[test]
fn minimum_duration_defers_supported_completion_and_timeout_does_not_fake_success() {
    for mode in modes() {
        let mut r = normal(mode);
        let command = start(&mut r);
        r.take_command().unwrap();
        r.submit_result(report(
            &command,
            1,
            r.now(),
            Some(true),
            true,
            Some(Status::Valid),
        ))
        .unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        r.advance_to(secs(2)).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Completed);
        let mut r = normal(mode);
        let command = start(&mut r);
        r.take_command().unwrap();
        r.advance_to(secs(10)).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Aborting);
        assert!(
            r.decisions()
                .iter()
                .any(|d| d.kind == DecisionKind::Timeout)
        );
        assert_eq!(
            r.submit_result(report(
                &command,
                1,
                r.now(),
                Some(true),
                true,
                Some(Status::Valid)
            )),
            Err(ActionError::SupersededCommand)
        );
        r.advance_to(secs(20)).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Failed);
    }
}
#[test]
fn invalid_results_are_atomic_and_progress_must_be_ordered() {
    for mode in modes() {
        let mut r = normal(mode);
        let command = start(&mut r);
        assert_eq!(
            r.submit_result(report(&command, 1, r.now(), Some(true), false, None)),
            Err(ActionError::UndeliveredCommand)
        );
        r.take_command().unwrap();
        r.advance_to(secs(1)).unwrap();
        r.submit_result(report(&command, 2, r.now(), None, false, None))
            .unwrap();
        let before = (
            r.assurance().evidence().clone(),
            r.audit().to_vec(),
            r.assurance().audit().to_vec(),
        );
        for bad in [
            report(&command, 2, r.now(), Some(true), true, None),
            report(&command, 1, r.now(), Some(true), true, None),
            report(&command, 3, secs(0), Some(true), true, None),
            report(&command, 3, secs(9), Some(true), true, None),
        ] {
            assert!(r.submit_result(bad).is_err());
        }
        let mut bad = report(&command, 3, r.now(), Some(true), true, None);
        bad.observed_state.insert("unbound".into(), Status::Valid);
        assert_eq!(r.submit_result(bad), Err(ActionError::InvalidResult));
        assert_eq!(
            (
                r.assurance().evidence().clone(),
                r.audit().to_vec(),
                r.assurance().audit().to_vec()
            ),
            before
        );
        assert_eq!(
            r.update(observation("a.ack", Status::Valid, 99, 100)),
            Err(ActionError::ReservedEvidence)
        );
    }
}
#[test]
fn recovery_policies_require_correlated_controller_success() {
    for mode in modes() {
        for (class, recovery, kind, final_state) in [
            (
                Interruptibility::CompletionSafe,
                RecoveryType::Reversible,
                CommandKind::CertifiedComplete,
                ActionState::Completed,
            ),
            (
                Interruptibility::Irreversible,
                RecoveryType::Irreversible,
                CommandKind::ForwardMitigation,
                ActionState::Failed,
            ),
            (
                Interruptibility::Preemptible,
                RecoveryType::Compensatable,
                CommandKind::SafeAbort,
                ActionState::Cancelled,
            ),
        ] {
            let mut r = runtime(mode, class, recovery);
            start(&mut r);
            if class == Interruptibility::Irreversible {
                let preparation = r.take_command().unwrap();
                r.submit_result(report(
                    &preparation.command_id,
                    1,
                    r.now(),
                    Some(true),
                    false,
                    None,
                ))
                .unwrap();
                let lease = r.request_lease("a", ActionPhase::Commit).unwrap();
                r.dispatch("a", ActionPhase::Commit, &lease).unwrap();
            }
            r.take_command().unwrap();
            r.advance_to(secs(2)).unwrap();
            r.update(observation("run_a", Status::Unknown, 2, 100))
                .unwrap();
            let command = r.take_command().unwrap();
            assert_eq!(command.kind, kind);
            r.update(observation("run_a", Status::Valid, 3, 100))
                .unwrap();
            assert!(!matches!(
                r.instances()["a"].state,
                ActionState::Executing | ActionState::Committed
            ));
            r.submit_result(report(
                &command.command_id,
                1,
                r.now(),
                Some(true),
                true,
                if kind == CommandKind::CertifiedComplete {
                    Some(Status::Valid)
                } else {
                    None
                },
            ))
            .unwrap();
            if recovery == RecoveryType::Compensatable {
                assert_eq!(r.instances()["a"].state, ActionState::Recovering);
                let recover = r.take_command().unwrap();
                assert_eq!(recover.kind, CommandKind::Recover);
                r.submit_result(report(
                    &recover.command_id,
                    1,
                    r.now(),
                    Some(true),
                    true,
                    None,
                ))
                .unwrap();
            }
            assert_eq!(r.instances()["a"].state, final_state);
        }
    }
}
#[test]
fn plan_change_preserves_running_history_and_aborts_uncommitted_irreversible_action() {
    for mode in modes() {
        let mut r = normal(mode);
        start(&mut r);
        r.change_plan_version("plan", 8).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        let mut r = runtime(
            mode,
            Interruptibility::Irreversible,
            RecoveryType::Irreversible,
        );
        start(&mut r);
        r.change_plan_version("plan", 8).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Aborting);
        assert_eq!(r.take_command().unwrap().kind, CommandKind::SafeAbort);
        assert_eq!(
            r.request_lease("a", ActionPhase::Commit),
            Err(ActionError::PlanVersion)
        );
    }
}
#[test]
fn bindings_are_exclusive_and_new_execution_resets_old_success() {
    for mode in modes() {
        let mut r = normal(mode);
        r.register_plan(Plan {
            plan_id: "retry".into(),
            version: 1,
            actions: BTreeMap::from([("retry-a".into(), "fan".into())]),
            precedence: BTreeSet::new(),
            created_at: secs(0),
        })
        .unwrap();
        let command = start(&mut r);
        assert_eq!(
            r.request_lease("retry-a", ActionPhase::Start),
            Err(ActionError::BindingsBusy)
        );
        r.take_command().unwrap();
        r.advance_to(secs(2)).unwrap();
        r.submit_result(report(
            &command,
            1,
            r.now(),
            Some(true),
            true,
            Some(Status::Valid),
        ))
        .unwrap();
        let lease = r.request_lease("retry-a", ActionPhase::Start).unwrap();
        r.dispatch("retry-a", ActionPhase::Start, &lease).unwrap();
        assert_eq!(
            r.assurance().evaluation().assurance_by_node["a.outcome"],
            Value::Unknown
        );
        assert_eq!(r.instances()["retry-a"].state, ActionState::Executing);
        assert!(r.request_lease("a", ActionPhase::Start).is_err());
    }
}
#[test]
fn configurations_and_plan_cycles_are_rejected_before_mutation() {
    let (graph, defs) = configuration(Interruptibility::Preemptible, RecoveryType::Reversible);
    for change in 0..5 {
        let mut defs = defs.clone();
        match change {
            0 => defs[0].controllers.safe_abort = false,
            1 => defs[0].start_contract = "missing".into(),
            2 => defs[0].min_duration = secs(11),
            3 => defs[0].max_start_lease = secs(0),
            _ => defs[0].outcomes.acknowledgement = "other".into(),
        }
        assert!(ActionRuntime::new(graph.clone(), defs).is_err());
    }
    let mut r = normal(EvaluationMode::Incremental);
    let before = r.instances().clone();
    let invalid = Plan {
        plan_id: "cycle".into(),
        version: 1,
        actions: BTreeMap::from([("x".into(), "fan".into()), ("y".into(), "follow".into())]),
        precedence: BTreeSet::from([("x".into(), "y".into()), ("y".into(), "x".into())]),
        created_at: secs(0),
    };
    assert!(r.register_plan(invalid).is_err());
    assert_eq!(r.instances(), &before);
}
#[test]
fn full_and_incremental_action_histories_match() {
    let mut all = Vec::new();
    for mode in modes() {
        let mut r = normal(mode);
        let old = r.request_lease("a", ActionPhase::Start).unwrap();
        r.update(observation("start", Status::Valid, 2, 100))
            .unwrap();
        assert!(r.dispatch("a", ActionPhase::Start, &old).is_err());
        start(&mut r);
        r.take_command().unwrap();
        r.update(observation("run_b", Status::Valid, 1, 4)).unwrap();
        r.update(observation("run_a", Status::Unknown, 2, 100))
            .unwrap();
        r.advance_to(secs(5)).unwrap();
        FakeActuator::respond_next(&mut r, Some(true), true, BTreeMap::new()).unwrap();
        all.push(r);
    }
    let a = &all[0];
    let b = &all[1];
    assert_eq!(a.instances(), b.instances());
    assert_eq!(a.leases(), b.leases());
    assert_eq!(a.commands(), b.commands());
    assert_eq!(a.decisions(), b.decisions());
    assert_eq!(a.audit(), b.audit());
    assert_eq!(a.assurance().audit(), b.assurance().audit());
}

#[test]
fn commit_cannot_skip_preparation_and_completion_cannot_skip_commit() {
    for mode in modes() {
        let mut r = runtime(
            mode,
            Interruptibility::Irreversible,
            RecoveryType::Irreversible,
        );
        assert_eq!(
            r.request_lease("a", ActionPhase::Commit),
            Err(ActionError::IllegalTransition)
        );
        let command = start(&mut r);
        assert_eq!(
            r.request_lease("a", ActionPhase::Commit),
            Err(ActionError::StartNotAcknowledged)
        );
        r.take_command().unwrap();
        r.advance_to(secs(2)).unwrap();
        r.submit_result(report(
            &command,
            1,
            r.now(),
            Some(true),
            true,
            Some(Status::Valid),
        ))
        .unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        let lease = r.request_lease("a", ActionPhase::Commit).unwrap();
        let commit = r.dispatch("a", ActionPhase::Commit, &lease).unwrap();
        assert_eq!(
            r.assurance().evaluation().assurance_by_node["a.outcome"],
            Value::Unknown
        );
        r.take_command().unwrap();
        r.submit_result(report(
            &commit,
            1,
            r.now(),
            Some(true),
            true,
            Some(Status::Valid),
        ))
        .unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Completed);
    }
}
#[test]
fn negative_controller_result_fails_and_unknown_controller_result_does_not_finish() {
    for mode in modes() {
        for class in [
            Interruptibility::Preemptible,
            Interruptibility::CompletionSafe,
        ] {
            let mut r = runtime(mode, class, RecoveryType::Reversible);
            start(&mut r);
            r.update(observation("run_b", Status::Invalid, 1, 100))
                .unwrap();
            r.update(observation("run_a", Status::Invalid, 2, 100))
                .unwrap();
            let control = r.take_command().unwrap();
            assert!(
                r.decisions()
                    .iter()
                    .any(|d| d.status == Some(Status::Invalid))
            );
            r.submit_result(report(&control.command_id, 1, r.now(), None, true, None))
                .unwrap();
            assert!(!r.instances()["a"].state.is_terminal());
            r.submit_result(report(
                &control.command_id,
                2,
                r.now(),
                Some(false),
                true,
                None,
            ))
            .unwrap();
            assert_eq!(r.instances()["a"].state, ActionState::Failed);
            assert_eq!(
                r.assurance().evaluation().assurance_by_node["a.outcome"],
                Value::Unknown
            );
        }
    }
}
#[test]
fn expired_outcome_and_acknowledgement_alone_do_not_establish_completion() {
    for mode in modes() {
        let mut r = normal(mode);
        let command = start(&mut r);
        r.take_command().unwrap();
        r.advance_to(secs(2)).unwrap();
        r.submit_result(report(&command, 1, r.now(), Some(true), true, None))
            .unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        let (graph, mut definitions) =
            configuration(Interruptibility::Preemptible, RecoveryType::Reversible);
        definitions[0].outcomes.max_age = secs(1);
        let mut r = ActionRuntime::with_mode(graph, definitions, mode).unwrap();
        for id in ["start", "run_a"] {
            r.update(observation(id, Status::Valid, 1, 100)).unwrap();
        }
        r.register_plan(Plan {
            plan_id: "p".into(),
            version: 1,
            actions: BTreeMap::from([("a".into(), "fan".into())]),
            precedence: BTreeSet::new(),
            created_at: secs(0),
        })
        .unwrap();
        let command = start(&mut r);
        r.take_command().unwrap();
        r.submit_result(report(
            &command,
            1,
            r.now(),
            Some(true),
            true,
            Some(Status::Valid),
        ))
        .unwrap();
        r.advance_to(secs(2)).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Executing);
        assert_eq!(
            r.assurance().evaluation().assurance_by_node["a.outcome"],
            Value::Unknown
        );
    }
}
#[test]
fn timeout_can_cascade_through_outcome_evidence_in_the_same_operation() {
    use dcra::graph::{AssuranceGraph, Justification, Node, NodeKind};
    for mode in modes() {
        let (graph, mut defs) =
            configuration(Interruptibility::Preemptible, RecoveryType::Reversible);
        let mut nodes: Vec<_> = graph
            .nodes()
            .iter()
            .map(|(id, kind)| Node::new(id, *kind))
            .collect();
        nodes.push(Node::new("b.run", NodeKind::ActionRun));
        let mut rules: Vec<_> = graph.justifications().values().cloned().collect();
        rules.push(Justification {
            justification_id: "b-running".into(),
            premises: vec!["a.outcome".into()],
            threshold: 1,
            conclusion: "b.run".into(),
        });
        defs[1].run_contract = Some("b.run".into());
        let mut r =
            ActionRuntime::with_mode(AssuranceGraph::new(nodes, rules).unwrap(), defs, mode)
                .unwrap();
        for id in ["start", "run_a"] {
            r.update(observation(id, Status::Valid, 1, 100)).unwrap();
        }
        r.register_plan(Plan {
            plan_id: "p".into(),
            version: 1,
            actions: BTreeMap::from([("a".into(), "fan".into()), ("b".into(), "follow".into())]),
            precedence: BTreeSet::new(),
            created_at: secs(0),
        })
        .unwrap();
        let a = start(&mut r);
        r.take_command().unwrap();
        r.advance_to(secs(1)).unwrap();
        r.submit_result(report(
            &a,
            1,
            r.now(),
            Some(true),
            false,
            Some(Status::Valid),
        ))
        .unwrap();
        let b = r.request_lease("b", ActionPhase::Start).unwrap();
        r.dispatch("b", ActionPhase::Start, &b).unwrap();
        r.take_command().unwrap();
        r.advance_to(secs(10)).unwrap();
        assert_eq!(r.instances()["a"].state, ActionState::Aborting);
        assert_eq!(r.instances()["b"].state, ActionState::Aborting);
        let b_abort = r
            .commands()
            .values()
            .find(|c| c.instance_id == "b" && c.kind == CommandKind::SafeAbort)
            .unwrap();
        assert_eq!(b_abort.issued_at, secs(10));
    }
}
#[test]
fn dispatch_at_a_simultaneous_evidence_deadline_is_rejected() {
    for mode in modes() {
        let mut r = normal(mode);
        r.update(observation("start", Status::Valid, 2, 2)).unwrap();
        let lease = r.request_lease("a", ActionPhase::Start).unwrap();
        r.advance_to(secs(2)).unwrap();
        assert!(r.dispatch("a", ActionPhase::Start, &lease).is_err());
        assert!(r.commands().is_empty());
        assert_eq!(r.instances()["a"].state, ActionState::Pending);
        let before = r.assurance().audit().to_vec();
        assert!(r.advance_to(secs(1)).is_err());
        assert_eq!(r.assurance().audit(), before);
    }
}

#[test]
fn both_modes_match_after_each_step_for_all_interruptibility_classes() {
    for class in [
        Interruptibility::Preemptible,
        Interruptibility::CompletionSafe,
        Interruptibility::Irreversible,
    ] {
        let recovery = if class == Interruptibility::Irreversible {
            RecoveryType::Irreversible
        } else {
            RecoveryType::Reversible
        };
        let mut full = runtime(EvaluationMode::Full, class, recovery);
        let mut incremental = runtime(EvaluationMode::Incremental, class, recovery);
        for step in 0..9 {
            for r in [&mut full, &mut incremental] {
                match step {
                    0 => {
                        r.request_lease("a", ActionPhase::Start).unwrap();
                    }
                    1 => {
                        let lease = r.leases().values().next().unwrap();
                        let token = LeaseToken {
                            lease_id: lease.lease_id.clone(),
                            nonce: lease.nonce.clone(),
                        };
                        r.update(observation("start", Status::Valid, 2, 100))
                            .unwrap();
                        assert!(r.dispatch("a", ActionPhase::Start, &token).is_err());
                    }
                    2 => {
                        start(r);
                    }
                    3 => {
                        let c = r.take_command().unwrap();
                        r.submit_result(report(&c.command_id, 1, r.now(), None, true, None))
                            .unwrap();
                    }
                    4 => {
                        let id = r.instances()["a"].active_command_id.clone().unwrap();
                        r.submit_result(report(
                            &id,
                            2,
                            r.now(),
                            Some(true),
                            false,
                            Some(Status::Valid),
                        ))
                        .unwrap();
                        if class == Interruptibility::Irreversible {
                            let lease = r.request_lease("a", ActionPhase::Commit).unwrap();
                            r.dispatch("a", ActionPhase::Commit, &lease).unwrap();
                            r.take_command().unwrap();
                        }
                    }
                    5 => {
                        r.advance_to(secs(2)).unwrap();
                        r.update(observation("run_a", Status::Unknown, 2, 100))
                            .unwrap();
                    }
                    6 => {
                        r.change_plan_version("plan", 8).unwrap();
                    }
                    7 => {
                        let c = r.take_command().unwrap();
                        r.submit_result(report(
                            &c.command_id,
                            1,
                            r.now(),
                            Some(true),
                            true,
                            if c.kind == CommandKind::CertifiedComplete {
                                Some(Status::Valid)
                            } else {
                                None
                            },
                        ))
                        .unwrap();
                    }
                    _ => {
                        r.advance_to(secs(25)).unwrap();
                    }
                }
            }
            assert_eq!(full.instances(), incremental.instances());
            assert_eq!(full.leases(), incremental.leases());
            assert_eq!(full.commands(), incremental.commands());
            assert_eq!(full.decisions(), incremental.decisions());
            assert_eq!(full.audit(), incremental.audit());
            assert_eq!(full.assurance().audit(), incremental.assurance().audit());
            assert_eq!(
                full.assurance().evaluation(),
                incremental.assurance().evaluation()
            );
            assert_eq!(
                full.assurance().evidence(),
                incremental.assurance().evidence()
            );
        }
    }
}
