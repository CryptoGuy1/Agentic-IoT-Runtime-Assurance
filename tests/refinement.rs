use dcra::actions::ActionState;
use dcra::model_v1::*;
fn options(policy: RefinementPolicy) -> RunOptions {
    RunOptions {
        policy,
        benchmark: true,
        ..RunOptions::default()
    }
}
#[test]
fn versioned_policies_keep_the_short_gap_difference_and_correlated_outcomes() {
    for policy in REFINEMENT_POLICIES {
        let report = run_closed_loop_with_options("short-gap", 42, options(policy)).unwrap();
        assert_eq!(
            report.start_allowed,
            matches!(
                policy,
                RefinementPolicy::B0
                    | RefinementPolicy::B1
                    | RefinementPolicy::B3
                    | RefinementPolicy::B6
            ),
            "{}",
            policy.id()
        );
        assert_eq!(report.score.false_completion_claims, 0);
    }
}
#[test]
fn wider_recovery_never_stops_required_ventilation_and_does_not_claim_original_completion() {
    let blocked =
        run_closed_loop_with_options("hazard-recovery", 42, RunOptions::default()).unwrap();
    assert!(!blocked.start_allowed);
    let report = run_closed_loop_with_options(
        "hazard-recovery",
        42,
        RunOptions {
            broader_recovery: true,
            ..RunOptions::default()
        },
    )
    .unwrap();
    assert!(report.start_allowed);
    assert!(
        report
            .score
            .commands
            .iter()
            .any(|c| c.action == Action::DoorOpen)
    );
    assert!(
        !report
            .score
            .commands
            .iter()
            .any(|c| c.action == Action::FanStop)
    );
    assert_eq!(report.final_state, ActionState::Cancelled);
    assert_eq!(report.score.false_completion_claims, 0);
}
#[test]
fn bounded_supervisor_recovers_then_installs_new_work_without_rewriting_started_history() {
    let report = run_closed_loop_with_options(
        "replanning",
        42,
        RunOptions {
            autonomous: true,
            ..RunOptions::default()
        },
    )
    .unwrap();
    assert_eq!(report.final_state, ActionState::Completed);
    assert!(
        !report
            .events_jsonl
            .contains("\"kind\":\"planner-budget-exhausted\"")
    );
    assert!(report.score.objective_achieved);
    let sequence: Vec<_> = report.score.commands.iter().map(|c| c.action).collect();
    assert_eq!(
        sequence,
        vec![
            Action::FanStart,
            Action::FanStop,
            Action::DoorOpen,
            Action::FanStart
        ]
    );
    assert_eq!(
        report
            .events_jsonl
            .matches("\"kind\":\"planner-plan-installed\"")
            .count(),
        1
    );
    assert!(
        report
            .events_jsonl
            .matches("\"kind\":\"planner-proposal\"")
            .count()
            <= 4
    );
    assert_eq!(report.score.false_completion_claims, 0);
}
#[test]
fn initial_uncertainty_can_be_retried_after_new_justified_information() {
    let report = run_closed_loop_with_options(
        "uncertain",
        42,
        RunOptions {
            autonomous: true,
            ..RunOptions::default()
        },
    )
    .unwrap();
    assert!(!report.start_allowed); // Initial fixed proposal is still recorded as withheld.
    assert_eq!(report.final_state, ActionState::Completed);
    assert_eq!(report.score.commands.len(), 1);
}
#[test]
fn persistent_effects_are_scored_after_the_initial_action_window() {
    let report =
        run_closed_loop_with_options("recovery", 42, options(RefinementPolicy::B0)).unwrap();
    assert!(report.score.objective_achieved);
    assert!(report.score.pressure_violation_ticks > 0);
    assert_eq!(report.total_ticks, 121);
    assert!(report.score.commands[0].physical_safe_in_window); // Initial window alone was insufficient.
}
#[test]
fn comparison_policy_contract_keeps_outcome_truth_separate_from_permission() {
    let info = BoundaryInformation {
        phase: DecisionBoundary::Outcome,
        phase_ready: true,
        permission: false,
        point_predicate: false,
        relevant_fresh: false,
        all_fresh: false,
        point_model: false,
        bounded_model: false,
        recovery_feasible: false,
        correlated_ack: true,
        observed_target: true,
    };
    for policy in REFINEMENT_POLICIES {
        assert!(policy_response(policy, info));
        assert!(!policy_response(
            policy,
            BoundaryInformation {
                correlated_ack: false,
                ..info
            }
        ));
    }
}
