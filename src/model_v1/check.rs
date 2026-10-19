use super::{estimate::predict, *};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Supported,
    Contradicted,
    Unknown,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CheckResult {
    pub decision: Decision,
    pub action: Action,
    pub checked_at: Duration,
    pub horizon_end: Duration,
    pub reasons: Vec<String>,
    pub observations: Vec<String>,
    pub pressure_envelope: Interval,
}

/// Checks one counterfactual applied-now command. Transport uncertainty of prior
/// commands remains represented. This is not an execution permission/lease API.
#[allow(clippy::too_many_arguments)]
pub fn check_action(
    state: &StateBox,
    assumptions: &Assumptions,
    initial: Inputs,
    commands: &[Command],
    observations: &[Observation],
    inconsistent: bool,
    action: Action,
) -> CheckResult {
    check_window(
        state,
        assumptions,
        initial,
        commands,
        observations,
        inconsistent,
        action,
        Some((Duration::ZERO, false)),
        action.horizon(),
    )
}

/// Shared conservative trajectory check. A missing candidate monitors existing inputs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_window(
    state: &StateBox,
    assumptions: &Assumptions,
    initial: Inputs,
    commands: &[Command],
    observations: &[Observation],
    inconsistent: bool,
    action: Action,
    delivery: Option<(Duration, bool)>,
    window: Duration,
) -> CheckResult {
    let mut result = CheckResult {
        decision: Decision::Supported,
        action,
        checked_at: state.at,
        horizon_end: state.at.saturating_add(window),
        reasons: vec![],
        observations: observations
            .iter()
            .filter(|o| o.quality == Quality::Trusted)
            .map(|o| o.id.clone())
            .collect(),
        pressure_envelope: state.pressure,
    };
    let valid_commands = commands.iter().all(|c| {
        c.id > 0
            && grid(c.issued_at)
            && grid(c.max_delay)
            && c.issued_at <= state.at
            && c.max_delay <= Duration::from_secs(60)
            && c.issued_at
                .checked_add(c.max_delay + Duration::from_millis(200))
                .is_some()
    }) && commands.windows(2).all(|pair| pair[0].id < pair[1].id);
    if inconsistent
        || !valid_commands
        || state.validate().is_err()
        || assumptions.validate().is_err()
        || initial.validate().is_err()
        || !grid(window)
        || window.is_zero()
        || window > Duration::from_secs(60)
        || delivery.is_some_and(|(delay, _)| {
            !grid(delay)
                || delay > Duration::from_secs(60)
                || state
                    .at
                    .checked_add(delay + Duration::from_millis(200))
                    .is_none()
        })
        || state.at.checked_add(window).is_none()
    {
        result.decision = Decision::Unknown;
        result
            .reasons
            .push("Inconsistent or invalid model inputs; repair/reset required.".into());
        return result;
    }
    let Some(next_id) = commands
        .iter()
        .map(|c| c.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
    else {
        result.decision = Decision::Unknown;
        result.reasons.push("Command identity exhausted.".into());
        return result;
    };
    let mut future_commands = commands.to_vec();
    if let Some((max_delay, may_drop)) = delivery {
        future_commands.push(Command {
            id: next_id,
            action,
            issued_at: state.at,
            max_delay,
            may_drop,
        });
    }
    let mut s = state.clone();
    let mut uncertain = false;
    let mut definite = false;
    loop {
        result.pressure_envelope = result.pressure_envelope.hull(s.pressure);
        if s.pressure.lo() < -0.80 || s.pressure.hi() > 0.80 {
            uncertain = true;
        }
        if s.pressure.lo() > 0.80 || s.pressure.hi() < -0.80 {
            definite = true;
        }
        // A closure request with possible occupancy is disallowed conservatively,
        // even if uncertain jamming could prevent the physical motion.
        if action == Action::DoorClose && s.occupied != BoolSet::False {
            uncertain = true;
        }
        if action == Action::DoorClose && s.occupied == BoolSet::True {
            definite = true;
        }
        if action == Action::FanStop && s.response_required != BoolSet::False {
            uncertain = true;
        }
        if action == Action::FanStop && s.response_required == BoolSet::True {
            definite = true;
        }
        if s.at == result.horizon_end {
            break;
        }
        s = predict(&s, assumptions, initial, &future_commands);
    }
    result.decision = if definite {
        Decision::Contradicted
    } else if uncertain {
        Decision::Unknown
    } else {
        Decision::Supported
    };
    result.reasons.push(match result.decision {
        Decision::Supported => "All represented states satisfy the configured constraints throughout the checked window. Physical success still requires observations.",
        Decision::Contradicted => "A configured constraint is certainly violated at a checked boundary.",
        Decision::Unknown => "The represented possibilities include a constraint violation; safety is not established.",
    }.into());
    if action == Action::DoorClose && s.occupied != BoolSet::False {
        result.reasons.push(
            "A recent empty-room reading alone does not establish continued clearance.".into(),
        );
    }
    result
}
