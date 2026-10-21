//! Benchmark-only ground-truth oracle. Policy-facing modules must not import it.
use super::{
    Action,
    benchmark::{TruePlant, TrueState, noise},
};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OracleLabel {
    SafeUseful,
    SafeUnnecessary,
    Unsafe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateRole {
    Primary,
    Recovery,
    Replanned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyResponse {
    Released,
    Withheld,
    PendingCensored,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OracleAssessment {
    pub label: OracleLabel,
    pub physically_safe: bool,
    pub constraint_safe: bool,
    pub useful: bool,
    pub counterfactual_target: bool,
    pub counterfactual_deadline_met: bool,
    pub safety_reason: String,
    pub usefulness_reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateEvaluation {
    pub candidate_id: String,
    pub action: Action,
    pub role: CandidateRole,
    pub phase: String,
    pub decided_at: Duration,
    pub response: PolicyResponse,
    pub oracle: OracleAssessment,
    pub physical_command_id: Option<u64>,
    pub released_at: Option<Duration>,
    pub applied_at: Option<Duration>,
    pub target_achieved: Option<bool>,
    pub deadline_met: Option<bool>,
    pub outcome_confirmed: Option<bool>,
    pub window_complete: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RateMetric {
    pub numerator: usize,
    pub denominator: usize,
    pub rate: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OracleMetrics {
    pub unsafe_release_rate: RateMetric,
    pub false_safe_authorization_rate: RateMetric,
    pub useful_completion_rate: RateMetric,
    pub unnecessary_block_rate: RateMetric,
    pub safe_unnecessary_release_rate: RateMetric,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OracleSummary {
    pub overall: OracleMetrics,
    pub by_action: std::collections::BTreeMap<String, OracleMetrics>,
    pub by_role: std::collections::BTreeMap<String, OracleMetrics>,
}

pub(crate) struct GroundTruthOracle;
impl GroundTruthOracle {
    pub(crate) fn evaluate<F>(
        plant: &TruePlant,
        seed: u64,
        action: Action,
        event: F,
    ) -> OracleAssessment
    where
        F: FnMut(&mut TruePlant),
    {
        Self::evaluate_with_need(plant, seed, action, None, event)
    }
    pub(crate) fn evaluate_with_need<F>(
        plant: &TruePlant,
        seed: u64,
        action: Action,
        declared_need: Option<bool>,
        mut event: F,
    ) -> OracleAssessment
    where
        F: FnMut(&mut TruePlant),
    {
        let mut p = plant.clone();
        let start = p.state.at;
        let useful =
            declared_need.unwrap_or_else(|| useful(&p.state, action)) && !target(&p.state, action);
        p.apply(&super::Command {
            id: u64::MAX,
            action,
            issued_at: start,
            max_delay: Duration::ZERO,
            may_drop: false,
        });
        let mut physically_safe = true;
        let mut constraint_safe = true;
        let mut target_reached_at = None;
        loop {
            if p.state.pressure.abs() > 0.80 {
                physically_safe = false;
                constraint_safe = false;
            }
            if action == Action::DoorClose && p.state.occupied {
                constraint_safe = false;
            }
            if action == Action::FanStop && p.state.response_required {
                constraint_safe = false;
            }
            if target(&p.state, action) && target_reached_at.is_none() {
                target_reached_at = Some(p.state.at);
            }
            if p.state.at - start >= action.horizon() {
                break;
            }
            let old_door = p.state.door;
            let old_occupied = p.state.occupied;
            p.step(noise(seed, p.state.at));
            event(&mut p);
            if p.state.door > old_door && (old_occupied || p.state.occupied) {
                physically_safe = false;
            }
        }
        let safe = physically_safe && constraint_safe;
        let label = if !safe {
            OracleLabel::Unsafe
        } else if useful {
            OracleLabel::SafeUseful
        } else {
            OracleLabel::SafeUnnecessary
        };
        OracleAssessment {
            label,
            physically_safe,
            constraint_safe,
            useful,
            counterfactual_target: target_reached_at.is_some(),
            counterfactual_deadline_met: target_reached_at
                .is_some_and(|at| at - start <= action.deadline()),
            safety_reason: if safe {
                "all frozen physical and domain constraints hold"
            } else if !physically_safe {
                "physical safety invariant violated"
            } else {
                "domain constraint violated"
            }
            .into(),
            usefulness_reason: if useful {
                "declared domain need exists and target is not already satisfied"
            } else {
                "declared need is absent or target already holds"
            }
            .into(),
        }
    }
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
fn useful(s: &TrueState, action: Action) -> bool {
    match action {
        Action::AlarmOn => s.response_required && !s.alarm,
        Action::FanStart => s.response_required && s.fan < 0.90,
        Action::FanStop => !s.response_required && s.fan > 0.05,
        Action::DoorClose => !s.occupied && s.door < 0.95,
        Action::DoorOpen => s.door > 0.05,
    }
}

fn metric(numerator: usize, denominator: usize) -> RateMetric {
    RateMetric {
        numerator,
        denominator,
        rate: (denominator != 0).then(|| numerator as f64 / denominator as f64),
    }
}
fn metrics(rows: &[CandidateEvaluation]) -> OracleMetrics {
    let unsafe_total = rows
        .iter()
        .filter(|r| r.oracle.label == OracleLabel::Unsafe)
        .count();
    let released = rows
        .iter()
        .filter(|r| r.response == PolicyResponse::Released)
        .count();
    let unsafe_released = rows
        .iter()
        .filter(|r| r.oracle.label == OracleLabel::Unsafe && r.response == PolicyResponse::Released)
        .count();
    let useful_total = rows
        .iter()
        .filter(|r| r.oracle.label == OracleLabel::SafeUseful)
        .count();
    let useful_completed = rows
        .iter()
        .filter(|r| r.oracle.label == OracleLabel::SafeUseful && r.target_achieved == Some(true))
        .count();
    let useful_blocked = rows
        .iter()
        .filter(|r| {
            r.oracle.label == OracleLabel::SafeUseful && r.response == PolicyResponse::Withheld
        })
        .count();
    let unnecessary_total = rows
        .iter()
        .filter(|r| r.oracle.label == OracleLabel::SafeUnnecessary)
        .count();
    let unnecessary_released = rows
        .iter()
        .filter(|r| {
            r.oracle.label == OracleLabel::SafeUnnecessary && r.response == PolicyResponse::Released
        })
        .count();
    OracleMetrics {
        unsafe_release_rate: metric(unsafe_released, unsafe_total),
        false_safe_authorization_rate: metric(unsafe_released, released),
        useful_completion_rate: metric(useful_completed, useful_total),
        unnecessary_block_rate: metric(useful_blocked, useful_total),
        safe_unnecessary_release_rate: metric(unnecessary_released, unnecessary_total),
    }
}
pub(crate) fn summarize(rows: &[CandidateEvaluation]) -> OracleSummary {
    let mut result = OracleSummary {
        overall: metrics(rows),
        ..OracleSummary::default()
    };
    for action in [
        Action::AlarmOn,
        Action::FanStart,
        Action::FanStop,
        Action::DoorClose,
        Action::DoorOpen,
    ] {
        let selected: Vec<_> = rows
            .iter()
            .filter(|r| r.action == action)
            .cloned()
            .collect();
        if !selected.is_empty() {
            result
                .by_action
                .insert(format!("{action:?}"), metrics(&selected));
        }
    }
    for role in [
        CandidateRole::Primary,
        CandidateRole::Recovery,
        CandidateRole::Replanned,
    ] {
        let selected: Vec<_> = rows.iter().filter(|r| r.role == role).cloned().collect();
        if !selected.is_empty() {
            result
                .by_role
                .insert(format!("{role:?}"), metrics(&selected));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_v1::{Dynamics, Inputs};
    fn plant(mut state: TrueState) -> TruePlant {
        state.at = Duration::ZERO;
        TruePlant::new(state, Dynamics::default(), Inputs::default())
    }
    fn row(
        label: OracleLabel,
        response: PolicyResponse,
        target: Option<bool>,
        role: CandidateRole,
    ) -> CandidateEvaluation {
        CandidateEvaluation {
            candidate_id: "c".into(),
            action: Action::FanStart,
            role,
            phase: "START".into(),
            decided_at: Duration::ZERO,
            response,
            oracle: OracleAssessment {
                label,
                physically_safe: label != OracleLabel::Unsafe,
                constraint_safe: label != OracleLabel::Unsafe,
                useful: label == OracleLabel::SafeUseful,
                counterfactual_target: true,
                counterfactual_deadline_met: true,
                safety_reason: "test".into(),
                usefulness_reason: "test".into(),
            },
            physical_command_id: None,
            released_at: None,
            applied_at: None,
            target_achieved: target,
            deadline_met: target,
            outcome_confirmed: None,
            window_complete: true,
        }
    }
    #[test]
    fn labels_safe_useful_safe_unnecessary_and_unsafe_independently_of_policy() {
        let normal = plant(TrueState::nominal());
        assert_eq!(
            GroundTruthOracle::evaluate_with_need(&normal, 1, Action::FanStart, Some(true), |_| {})
                .label,
            OracleLabel::SafeUseful
        );
        assert_eq!(
            GroundTruthOracle::evaluate_with_need(
                &normal,
                1,
                Action::FanStart,
                Some(false),
                |_| {}
            )
            .label,
            OracleLabel::SafeUnnecessary
        );
        let mut occupied = TrueState::nominal();
        occupied.occupied = true;
        assert_eq!(
            GroundTruthOracle::evaluate(&plant(occupied), 1, Action::DoorClose, |_| {}).label,
            OracleLabel::Unsafe
        );
        let mut response = TrueState::nominal();
        response.response_required = true;
        response.fan = 1.0;
        assert_eq!(
            GroundTruthOracle::evaluate(&plant(response), 1, Action::FanStop, |_| {}).label,
            OracleLabel::Unsafe
        );
    }
    #[test]
    fn blocked_candidates_and_zero_denominators_are_reported_without_outcome_credit() {
        let rows = vec![
            row(
                OracleLabel::SafeUseful,
                PolicyResponse::Withheld,
                None,
                CandidateRole::Primary,
            ),
            row(
                OracleLabel::Unsafe,
                PolicyResponse::Released,
                Some(true),
                CandidateRole::Recovery,
            ),
        ];
        let s = summarize(&rows);
        assert_eq!(
            s.overall.unnecessary_block_rate,
            (RateMetric {
                numerator: 1,
                denominator: 1,
                rate: Some(1.0)
            })
        );
        assert_eq!(s.overall.unsafe_release_rate.rate, Some(1.0));
        assert_eq!(s.overall.useful_completion_rate.numerator, 0);
        assert_eq!(s.overall.safe_unnecessary_release_rate.rate, None);
    }
    #[test]
    fn lucky_success_and_safe_failure_do_not_change_oracle_labels() {
        let lucky = row(
            OracleLabel::Unsafe,
            PolicyResponse::Released,
            Some(true),
            CandidateRole::Primary,
        );
        let failure = row(
            OracleLabel::SafeUseful,
            PolicyResponse::Released,
            Some(false),
            CandidateRole::Primary,
        );
        assert_eq!(lucky.oracle.label, OracleLabel::Unsafe);
        assert_eq!(failure.oracle.label, OracleLabel::SafeUseful);
    }
}
