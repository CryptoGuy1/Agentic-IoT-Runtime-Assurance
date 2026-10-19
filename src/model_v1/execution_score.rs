//! Post-run scalar scoring. Never imports the estimator, model gate or runtime.
use super::{Action, benchmark::TrueState};
use std::time::Duration;

#[derive(Clone, Debug)]
pub(crate) struct ExecutionBoundary {
    pub id: u64,
    pub action: Action,
    pub released_at: Duration,
    pub applied_at: Option<Duration>,
    pub accepted_completion_at: Option<Duration>,
}
#[derive(Clone, Debug)]
pub(crate) struct CompletionClaim {
    pub id: u64,
    pub claimed_at: Duration,
    pub observed_at: Duration,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AppliedCommandScore {
    pub id: u64,
    pub action: Action,
    pub released_at: Duration,
    pub applied_at: Option<Duration>,
    pub physical_target_at: Option<Duration>,
    /// From physical release, including command transit. None means censored.
    pub target_by_deadline: Option<bool>,
    pub useful_at_release: bool,
    pub window_complete: bool,
    pub physical_safe_in_window: bool,
    pub constraint_safe_in_window: bool,
    pub feedback_confirmed_at: Option<Duration>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionScore {
    pub pressure_violation_ticks: usize,
    pub occupied_closing_motion_ticks: usize,
    pub initial_pressure_violation: bool,
    pub false_completion_claims: usize,
    pub physical_targets_without_confirmation: usize,
    pub objective_achieved: bool,
    pub commands: Vec<AppliedCommandScore>,
}
fn target(s: &TrueState, action: Action) -> bool {
    match action {
        Action::AlarmOn => s.alarm,
        Action::FanStart => s.fan >= 0.90,
        Action::FanStop => s.fan <= 0.05,
        Action::DoorClose => s.door >= 0.95,
        Action::DoorOpen => s.door <= 0.05,
    }
}
/// Scores actual recorded trajectories, without cloning/applying counterfactual
/// commands. Physical safety, configured constraints, targets and confirmation
/// are distinct. Usefulness is the predeclared target not already being met.
pub(crate) fn score_execution(
    states: &[TrueState],
    boundaries: &[ExecutionBoundary],
    completion_claims: &[CompletionClaim],
    objective: Action,
) -> ExecutionScore {
    assert!(!states.is_empty());
    let end = states.last().unwrap().at;
    let moving_occupied =
        |old: &TrueState, new: &TrueState| new.door > old.door && (old.occupied || new.occupied);
    let pressure_violation_ticks = states.iter().filter(|s| s.pressure.abs() > 0.80).count();
    let occupied_closing_motion_ticks = states
        .windows(2)
        .filter(|pair| moving_occupied(&pair[0], &pair[1]))
        .count();
    let commands: Vec<_> = boundaries
        .iter()
        .map(|command| {
            let window_end = command.released_at + command.action.horizon();
            let deadline = command.released_at + command.action.deadline();
            let initial = states.iter().find(|s| s.at == command.released_at).unwrap();
            let superseded_at = boundaries
                .iter()
                .filter(|other| {
                    other.id > command.id && other.action.device() == command.action.device()
                })
                .filter_map(|other| other.applied_at)
                .min();
            // No target credit before receipt/application, even if it already held.
            let physical_target_at = command.applied_at.and_then(|applied| {
                states
                    .iter()
                    .find(|s| {
                        s.at >= applied
                            && s.at <= window_end
                            && superseded_at.is_none_or(|until| s.at < until)
                            && target(s, command.action)
                    })
                    .map(|s| s.at)
            });
            let target_by_deadline = if physical_target_at.is_some_and(|at| at <= deadline) {
                Some(true)
            } else if end >= deadline {
                Some(false)
            } else {
                None
            };
            let pressure_safe = states
                .iter()
                .filter(|s| s.at >= command.released_at && s.at <= window_end)
                .all(|s| s.pressure.abs() <= 0.80);
            let motion_safe = states
                .windows(2)
                .filter(|p| p[1].at > command.released_at && p[1].at <= window_end)
                .all(|p| !moving_occupied(&p[0], &p[1]));
            let contract_safe = states
                .iter()
                .filter(|s| s.at >= command.released_at && s.at <= window_end)
                .all(|s| {
                    (command.action != Action::DoorClose || !s.occupied)
                        && (command.action != Action::FanStop || !s.response_required)
                });
            AppliedCommandScore {
                id: command.id,
                action: command.action,
                released_at: command.released_at,
                applied_at: command.applied_at,
                physical_target_at,
                target_by_deadline,
                useful_at_release: !target(initial, command.action),
                window_complete: end >= window_end,
                physical_safe_in_window: pressure_safe && motion_safe,
                constraint_safe_in_window: pressure_safe && motion_safe && contract_safe,
                feedback_confirmed_at: command.accepted_completion_at,
            }
        })
        .collect();
    let false_completion_claims = completion_claims
        .iter()
        .filter(|claim| {
            let command = boundaries.iter().find(|c| c.id == claim.id);
            command.is_none_or(|c| {
                c.applied_at.is_none_or(|at| at > claim.observed_at)
                    || boundaries.iter().any(|other| {
                        other.id > c.id
                            && other.action.device() == c.action.device()
                            && other.applied_at.is_some_and(|at| at <= claim.observed_at)
                    })
                    || claim.observed_at > claim.claimed_at
                    || states
                        .iter()
                        .find(|s| s.at == claim.observed_at)
                        .is_none_or(|s| !target(s, c.action))
            })
        })
        .count();
    ExecutionScore {
        pressure_violation_ticks,
        occupied_closing_motion_ticks,
        initial_pressure_violation: states[0].pressure.abs() > 0.80,
        false_completion_claims,
        physical_targets_without_confirmation: commands
            .iter()
            .filter(|c| c.physical_target_at.is_some() && c.feedback_confirmed_at.is_none())
            .count(),
        objective_achieved: target(states.last().unwrap(), objective),
        commands,
    }
}

#[cfg(test)]
mod tests {
    use super::super::benchmark::TrueState;
    use super::*;
    fn state(ms: u64, fan: f64) -> TrueState {
        let mut s = TrueState::nominal();
        s.at = Duration::from_millis(ms);
        s.fan = fan;
        s.response_required = false;
        s
    }
    #[test]
    fn scorer_separates_measured_success_from_confirmation_and_detects_false_completion() {
        let states = vec![
            state(0, 0.0),
            state(100, 0.1),
            state(4000, 0.95),
            state(6000, 0.98),
        ];
        let boundary = ExecutionBoundary {
            id: 1,
            action: Action::FanStart,
            released_at: Duration::ZERO,
            applied_at: Some(Duration::from_millis(100)),
            accepted_completion_at: None,
        };
        let score = score_execution(
            &states,
            &[boundary],
            &[CompletionClaim {
                id: 1,
                claimed_at: Duration::from_millis(100),
                observed_at: Duration::from_millis(100),
            }],
            Action::FanStart,
        );
        assert_eq!(score.commands[0].target_by_deadline, Some(true));
        assert_eq!(score.false_completion_claims, 1);
        assert_eq!(score.physical_targets_without_confirmation, 1);
        assert!(score.objective_achieved);
    }
    #[test]
    fn no_application_means_no_target_credit_and_short_runs_are_censored() {
        let states = vec![state(0, 0.95), state(100, 0.95)];
        let score = score_execution(
            &states,
            &[ExecutionBoundary {
                id: 1,
                action: Action::FanStart,
                released_at: Duration::ZERO,
                applied_at: None,
                accepted_completion_at: None,
            }],
            &[],
            Action::FanStart,
        );
        assert_eq!(score.commands[0].physical_target_at, None);
        assert_eq!(score.commands[0].target_by_deadline, None);
        assert!(!score.commands[0].useful_at_release);
    }
}

#[cfg(test)]
mod violation_tests {
    use super::*;
    #[test]
    fn scorer_records_initial_environmental_violation_motion_and_missed_deadline_separately() {
        let mut first = TrueState::nominal();
        first.pressure = 0.90;
        first.door = 0.0;
        first.occupied = true;
        let mut next = first.clone();
        next.at = Duration::from_millis(100);
        next.pressure = 0.85;
        next.door = 0.1;
        let mut last = next.clone();
        last.at = Duration::from_secs(4);
        last.pressure = 0.10;
        last.door = 0.2;
        let score = score_execution(
            &[first, next, last],
            &[ExecutionBoundary {
                id: 1,
                action: Action::DoorClose,
                released_at: Duration::ZERO,
                applied_at: Some(Duration::from_millis(100)),
                accepted_completion_at: None,
            }],
            &[],
            Action::DoorClose,
        );
        assert!(score.initial_pressure_violation);
        assert_eq!(score.pressure_violation_ticks, 2);
        assert_eq!(score.occupied_closing_motion_ticks, 2);
        assert_eq!(score.commands[0].target_by_deadline, Some(false));
        assert!(!score.commands[0].physical_safe_in_window);
        assert!(!score.commands[0].constraint_safe_in_window);
        assert!(score.commands[0].window_complete);
    }
}

#[cfg(test)]
mod supersession_tests {
    use super::*;
    #[test]
    fn later_target_cannot_be_credited_to_a_superseded_command() {
        let mut first = TrueState::nominal();
        first.fan = 0.0;
        let mut next = first.clone();
        next.at = Duration::from_millis(100);
        let mut target = next.clone();
        target.at = Duration::from_millis(200);
        target.fan = 0.95;
        let boundaries = [
            ExecutionBoundary {
                id: 1,
                action: Action::FanStart,
                released_at: Duration::ZERO,
                applied_at: Some(Duration::ZERO),
                accepted_completion_at: None,
            },
            ExecutionBoundary {
                id: 2,
                action: Action::FanStart,
                released_at: Duration::from_millis(100),
                applied_at: Some(Duration::from_millis(100)),
                accepted_completion_at: None,
            },
        ];
        let score = score_execution(&[first, next, target], &boundaries, &[], Action::FanStart);
        assert_eq!(score.commands[0].physical_target_at, None);
        assert_eq!(
            score.commands[1].physical_target_at,
            Some(Duration::from_millis(200))
        );
    }
}
