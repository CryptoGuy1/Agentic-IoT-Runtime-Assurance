use dcra::actions::ActionState;
use dcra::model_v1::{Action, CLOSED_LOOP_SCENARIOS, run_closed_loop};
use std::time::Duration;
#[test]
fn normal_execution_is_applied_measured_and_confirmed_without_synthetic_completion() {
    let report = run_closed_loop("normal", 42).unwrap();
    assert!(report.start_allowed);
    assert_eq!(report.final_state, ActionState::Completed);
    assert_eq!(report.contained_ticks, report.total_ticks);
    assert_eq!(report.score.commands.len(), 1);
    let command = &report.score.commands[0];
    assert_eq!(command.action, Action::FanStart);
    assert!(command.applied_at.unwrap() > command.released_at);
    assert!(command.physical_target_at.unwrap() > command.applied_at.unwrap());
    assert!(command.feedback_confirmed_at.unwrap() > command.physical_target_at.unwrap());
    assert_eq!(command.target_by_deadline, Some(true));
    assert_eq!(report.score.false_completion_claims, 0);
    assert_eq!(report.score.physical_targets_without_confirmation, 0);
    assert!(report.score.objective_achieved);
}
#[test]
fn short_gap_uses_a_model_claim_and_still_executes_the_real_simulated_command() {
    let report = run_closed_loop("short-gap", 42).unwrap();
    assert!(report.start_allowed);
    assert_eq!(report.final_state, ActionState::Completed);
    assert!(
        report
            .json
            .contains("\"direct_pressure_status_at_proposal\":\"Unknown\"")
    );
    assert!(report.score.commands[0].physical_target_at.is_some());
    assert_eq!(report.contained_ticks, report.total_ticks);
}
#[test]
fn predicted_risk_drives_a_measured_recovery_without_claiming_original_success() {
    let report = run_closed_loop("recovery", 42).unwrap();
    assert!(report.start_allowed);
    assert_eq!(report.final_state, ActionState::Cancelled);
    assert_eq!(report.score.commands.len(), 2);
    assert_eq!(report.score.commands[0].action, Action::FanStart);
    assert_eq!(report.score.commands[0].target_by_deadline, Some(false));
    assert_eq!(report.score.commands[1].action, Action::FanStop);
    assert_eq!(report.score.commands[1].target_by_deadline, Some(true));
    assert!(report.score.commands[1].feedback_confirmed_at.is_some());
    assert_eq!(report.score.pressure_violation_ticks, 0);
    assert!(!report.score.objective_achieved);
}
#[test]
fn physical_success_with_lost_feedback_remains_unconfirmed() {
    let report = run_closed_loop("lost-feedback", 42).unwrap();
    assert_eq!(report.final_state, ActionState::Executing);
    assert!(report.score.objective_achieved);
    assert_eq!(report.score.commands[0].target_by_deadline, Some(true));
    assert_eq!(report.score.commands[0].feedback_confirmed_at, None);
    assert_eq!(report.score.physical_targets_without_confirmation, 1);
    assert_eq!(report.score.false_completion_claims, 0);
}
#[test]
fn uncertainty_can_withhold_a_truth_safe_initial_opportunity() {
    let report = run_closed_loop("uncertain", 42).unwrap();
    assert!(!report.start_allowed);
    assert!(report.score.commands.is_empty());
    assert!(!report.score.initial_pressure_violation);
    assert!(!report.score.objective_achieved);
}
#[test]
fn delays_are_real_transit_not_a_shifted_completion_label() {
    let report = run_closed_loop("delayed-command", 42).unwrap();
    let command = &report.score.commands[0];
    assert_eq!(
        command.applied_at.unwrap() - command.released_at,
        Duration::from_millis(600)
    );
    // The delay also reduces recovery headroom: this fixture may recover
    // before the original target, rather than pretending delivery was immediate.
    assert!(
        report
            .score
            .commands
            .iter()
            .any(|c| c.action == Action::FanStop)
    );
    assert_eq!(report.final_state, ActionState::Cancelled);
    let delayed = run_closed_loop("delayed-reading", 42).unwrap();
    assert!(delayed.start_allowed);
    assert_eq!(delayed.final_state, ActionState::Completed);
    assert_eq!(delayed.contained_ticks, delayed.total_ticks);
}
#[test]
fn all_scenarios_replay_exactly_and_preserve_declared_containment() {
    for scenario in CLOSED_LOOP_SCENARIOS {
        let a = run_closed_loop(scenario, 7).unwrap();
        let b = run_closed_loop(scenario, 7).unwrap();
        assert_eq!(a.json, b.json);
        assert_eq!(a.events_jsonl, b.events_jsonl);
        assert_eq!(a.score, b.score);
        assert_eq!(a.contained_ticks, a.total_ticks, "{scenario}");
        assert_eq!(a.score.false_completion_claims, 0);
    }
    assert!(run_closed_loop("unknown", 42).is_err());
}
