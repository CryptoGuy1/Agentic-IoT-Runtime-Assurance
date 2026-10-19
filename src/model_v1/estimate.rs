use super::{interval::p, *};
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    Applied,
    Duplicate,
    Superseded,
    Expired,
    Missing,
    Inconsistent,
}

/// Policy-visible inputs only. No reference to the benchmark plant or scorer.
#[derive(Clone, Debug)]
pub struct Estimator {
    initial: StateBox,
    initial_inputs: Inputs,
    assumptions: Assumptions,
    state: StateBox,
    observations: Vec<Observation>,
    seen: BTreeMap<String, Observation>,
    sources: BTreeMap<(String, Variable), (u64, Duration)>,
    commands: Vec<Command>,
    inconsistent: bool,
    revision: u64,
}
impl Estimator {
    pub fn new(
        initial: StateBox,
        inputs: Inputs,
        assumptions: Assumptions,
    ) -> Result<Self, String> {
        initial.validate()?;
        inputs.validate()?;
        assumptions.validate()?;
        Ok(Self {
            state: initial.clone(),
            initial,
            initial_inputs: inputs,
            assumptions,
            observations: vec![],
            seen: BTreeMap::new(),
            sources: BTreeMap::new(),
            commands: vec![],
            inconsistent: false,
            revision: 1,
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn initial_inputs(&self) -> Inputs {
        self.initial_inputs
    }
    pub fn state(&self) -> &StateBox {
        &self.state
    }
    pub fn assumptions(&self) -> &Assumptions {
        &self.assumptions
    }
    pub fn inconsistent(&self) -> bool {
        self.inconsistent
    }
    /// All accepted messages, including missing/inconsistent quality, in ID order.
    pub fn accepted_observations(&self) -> impl Iterator<Item = &Observation> {
        self.seen.values()
    }
    pub fn initial_state(&self) -> &StateBox {
        &self.initial
    }
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }
    pub fn advance_to(&mut self, at: Duration) -> Result<(), String> {
        if !grid(at)
            || at < self.state.at
            || at.saturating_sub(self.initial.at) > Duration::from_secs(3600)
        {
            return Err("time must advance on the 100 ms grid within a one-hour experiment".into());
        }
        if self.state.at < at {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or("model revision overflow")?;
        }
        while self.state.at < at {
            self.state = predict(
                &self.state,
                &self.assumptions,
                self.initial_inputs,
                &self.commands,
            );
        }
        Ok(())
    }
    pub fn note_command(&mut self, command: Command) -> Result<(), String> {
        if command.id == 0
            || command.issued_at != self.state.at
            || !grid(command.max_delay)
            || command.max_delay > Duration::from_secs(60)
            || self.commands.last().is_some_and(|c| c.id >= command.id)
            || command
                .issued_at
                .checked_add(command.max_delay + Duration::from_millis(200))
                .is_none()
        {
            return Err(
                "command requires a new increasing ID, current issue time and bounded grid delay"
                    .into(),
            );
        }
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or("model revision overflow")?;
        self.commands.push(command);
        Ok(())
    }
    pub fn observe(&mut self, obs: Observation) -> Result<Update, String> {
        obs.validate(self.state.at)?;
        if obs.generated_at < self.initial.at {
            return Err("observation predates estimator initialization".into());
        }
        if let Some(old) = self.seen.get(&obs.id) {
            // Transport duplication changes receipt time, not the source message.
            let mut same = obs.clone();
            same.received_at = old.received_at;
            return if *old == same {
                Ok(Update::Duplicate)
            } else {
                Err("observation ID reused with a different payload".into())
            };
        }
        let key = (obs.source.clone(), obs.variable);
        if let Some((version, generated)) = self.sources.get(&key) {
            if obs.version <= *version {
                return Ok(Update::Superseded);
            }
            if obs.generated_at < *generated {
                return Err("new source version has an older timestamp".into());
            }
        }
        if self.state.at >= obs.generated_at + obs.max_age {
            return Ok(Update::Expired);
        }
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or("model revision overflow")?;
        self.seen.insert(obs.id.clone(), obs.clone());
        self.sources.insert(key, (obs.version, obs.generated_at));
        match obs.quality {
            Quality::Missing => Ok(Update::Missing),
            Quality::Inconsistent => {
                self.inconsistent = true;
                Ok(Update::Inconsistent)
            }
            Quality::Trusted => {
                self.observations.push(obs);
                self.observations.sort_by(|a, b| {
                    (a.generated_at, &a.source, a.version, &a.id).cmp(&(
                        b.generated_at,
                        &b.source,
                        b.version,
                        &b.id,
                    ))
                });
                self.rebuild();
                Ok(if self.inconsistent {
                    Update::Inconsistent
                } else {
                    Update::Applied
                })
            }
        }
    }
    fn rebuild(&mut self) {
        let now = self.state.at;
        let mut state = self.initial.clone();
        let mut i = 0;
        loop {
            while i < self.observations.len() && self.observations[i].generated_at == state.at {
                if let Some(updated) = constrain(&state, &self.observations[i]) {
                    state = updated;
                } else {
                    // Keep a nonempty conservative prediction; never authorize from it while inconsistent.
                    self.inconsistent = true;
                }
                i += 1;
            }
            if state.at == now {
                break;
            }
            state = predict(
                &state,
                &self.assumptions,
                self.initial_inputs,
                &self.commands,
            );
        }
        self.state = state;
    }
    /// Delivery-aware candidate safety; this does not establish physical success.
    pub fn check_point_delivery(&self, action: Action, delay: Duration) -> CheckResult {
        let state = self.state.point_projection();
        let mut assumptions = self.assumptions.clone();
        assumptions.disturbance_scale = 0.0;
        assumptions.faults_may_start = false;
        assumptions.source =
            Interval::point(assumptions.source.midpoint()).expect("validated midpoint");
        super::check::check_window(
            &state,
            &assumptions,
            self.initial_inputs,
            &self.commands,
            &self.observations,
            self.inconsistent,
            action,
            Some((delay, false)),
            action.horizon(),
        )
    }
    pub fn check_delivery(&self, action: Action, delay: Duration, may_drop: bool) -> CheckResult {
        super::check::check_window(
            &self.state,
            &self.assumptions,
            self.initial_inputs,
            &self.commands,
            &self.observations,
            self.inconsistent,
            action,
            Some((delay, may_drop)),
            action.horizon(),
        )
    }
    /// Monitor the existing command history without pretending to resend a command.
    pub fn check_continuation(&self, action: Action) -> CheckResult {
        super::check::check_window(
            &self.state,
            &self.assumptions,
            self.initial_inputs,
            &self.commands,
            &self.observations,
            self.inconsistent,
            action,
            None,
            action.horizon(),
        )
    }
    /// Check recovery from the enclosure at the next monitoring boundary, including
    /// commands already in transit. This is a finite feasibility test, not a proof
    /// of indefinite recoverability. The target must be enclosed by its deadline.
    pub fn check_recovery(
        &self,
        candidate: Option<Command>,
        recovery: Action,
        delay: Duration,
        may_drop: bool,
    ) -> Result<CheckResult, String> {
        let mut future = self.clone();
        if let Some(command) = candidate {
            future.note_command(command)?;
        }
        future.advance_to(future.state.at.checked_add(TICK).ok_or("time overflow")?)?;
        future.check_recovery_delivery(recovery, delay, may_drop)
    }
    /// Safety and target feasibility for a recovery released at the current time.
    pub fn check_recovery_delivery(
        &self,
        recovery: Action,
        delay: Duration,
        may_drop: bool,
    ) -> Result<CheckResult, String> {
        let mut future = self.clone();
        let mut check = future.check_delivery(recovery, delay, may_drop);
        let command = Command {
            id: future
                .commands
                .last()
                .map_or(Some(1), |c| c.id.checked_add(1))
                .ok_or("command IDs exhausted")?,
            action: recovery,
            issued_at: future.state.at,
            max_delay: delay,
            may_drop,
        };
        future.note_command(command)?;
        future.advance_to(
            future
                .state
                .at
                .checked_add(recovery.deadline())
                .ok_or("time overflow")?,
        )?;
        let target = match recovery {
            Action::AlarmOn => future.state.alarm == BoolSet::True,
            Action::FanStart => future.state.fan.lo() >= 0.90,
            Action::FanStop => future.state.fan.hi() <= 0.05,
            Action::DoorClose => future.state.door.lo() >= 0.95,
            Action::DoorOpen => future.state.door.hi() <= 0.05,
        };
        if !target {
            if check.decision == Decision::Supported {
                check.decision = Decision::Unknown;
            }
            check.reasons.push("Recovery target is not guaranteed by its deadline under the declared delivery and fault bounds.".into());
        }
        Ok(check)
    }
    pub fn check(&self, action: Action) -> CheckResult {
        check_action(
            &self.state,
            &self.assumptions,
            self.initial_inputs,
            &self.commands,
            &self.observations,
            self.inconsistent,
            action,
        )
    }
}

fn constrain(state: &StateBox, obs: &Observation) -> Option<StateBox> {
    let mut s = state.clone();
    match (obs.variable, obs.value) {
        (Variable::Occupancy, Reading::Boolean(v)) => s.occupied = s.occupied.intersect(v)?,
        (Variable::Alarm, Reading::Boolean(v)) => s.alarm = s.alarm.intersect(v)?,
        (variable, Reading::Number(value)) => {
            let bounds = Interval::around(value, obs.error?).clip(
                if variable == Variable::Pressure {
                    -1.0
                } else {
                    0.0
                },
                1.0,
            );
            let target = match variable {
                Variable::Hazard => &mut s.hazard,
                Variable::Pressure => &mut s.pressure,
                Variable::Fan => &mut s.fan,
                Variable::Door => &mut s.door,
                _ => return None,
            };
            *target = target.intersect(bounds)?;
        }
        _ => return None,
    }
    s.refresh_response();
    Some(s)
}

/// Possible actuator inputs under monotonically ordered device command IDs.
pub(crate) fn input_range(
    at: Duration,
    device: u8,
    initial: Inputs,
    commands: &[Command],
) -> Interval {
    let lag = if device == 0 {
        Duration::from_millis(200)
    } else {
        Duration::ZERO
    };
    let relevant: Vec<_> = commands
        .iter()
        .filter(|c| c.action.device() == device && c.issued_at + lag <= at)
        .collect();
    let guaranteed = relevant
        .iter()
        .filter(|c| !c.may_drop && c.issued_at + c.max_delay + lag <= at)
        .max_by_key(|c| c.id);
    let value = |a: Action| match a {
        Action::AlarmOn | Action::FanStart | Action::DoorClose => 1.0,
        Action::FanStop => 0.0,
        Action::DoorOpen => -1.0,
    };
    let mut range = p(guaranteed.map_or(
        match device {
            0 => f64::from(initial.alarm),
            1 => f64::from(initial.fan),
            _ => f64::from(initial.door),
        },
        |c| value(c.action),
    ));
    for c in &relevant {
        if guaranteed.is_none_or(|g| c.id > g.id) {
            range = range.hull(p(value(c.action)));
        }
    }
    range
}

pub(crate) fn predict(
    state: &StateBox,
    assumptions: &Assumptions,
    initial: Inputs,
    commands: &[Command],
) -> StateBox {
    let mut s = state.clone();
    let q = &assumptions.dynamics;
    let dt = p(0.1);
    let disturbance = |bound: f64| Interval::around(0.0, bound * assumptions.disturbance_scale);
    let uf = input_range(state.at, 1, initial, commands);
    let ud = input_range(state.at, 2, initial, commands);
    if assumptions.faults_may_start {
        if s.fan_failed == BoolSet::False {
            s.fan_failed = BoolSet::Either;
        }
        if s.door_jammed == BoolSet::False {
            s.door_jammed = BoolSet::Either;
        }
    }
    s.hazard = state
        .hazard
        .mul(p(1.0).sub(dt.mul(p(q.hazard_decay).add(p(q.fan_removal).mul(state.fan)))))
        .add(dt.mul(assumptions.source.add(disturbance(0.01))))
        .clip(0.0, 1.0);
    // Enclose division rounding too; positive validated tau avoids singularities.
    let rate = Interval::around(0.1 / q.fan_tau, 0.0);
    let healthy = state
        .fan
        .mul(p(1.0).sub(rate))
        .add(uf.mul(rate))
        .add(dt.mul(disturbance(0.02)))
        .clip(0.0, 1.0);
    let failed = state
        .fan
        .sub(Interval::around(0.1 / 0.8, 0.0))
        .clip(0.0, 1.0);
    s.fan = match s.fan_failed {
        BoolSet::False => healthy,
        BoolSet::True => failed,
        BoolSet::Either => healthy.hull(failed),
    };
    s.pressure = state
        .pressure
        .mul(p(1.0).sub(dt.mul(p(q.pressure_decay))))
        .add(
            dt.mul(
                p(q.pressure_coupling)
                    .mul(state.fan)
                    .mul(state.door)
                    .add(disturbance(0.02)),
            ),
        )
        .clip(-1.0, 1.0);
    let moving = state
        .door
        .add(dt.mul(p(q.door_speed).mul(ud).add(disturbance(0.01))))
        .clip(0.0, 1.0);
    s.door = match s.door_jammed {
        BoolSet::False => moving,
        BoolSet::True => state.door,
        BoolSet::Either => moving.hull(state.door),
    };
    s.at += TICK;
    // Occupancy can change at the end of this step, including the protection boundary.
    if s.at >= assumptions.occupancy_stable_until {
        s.occupied = BoolSet::Either;
    }
    let alarm = input_range(s.at, 0, initial, commands);
    if alarm.lo() == 1.0 {
        s.alarm = BoolSet::True;
    } else if alarm.hi() == 1.0 && s.alarm != BoolSet::True {
        s.alarm = BoolSet::Either;
    }
    s.refresh_response();
    s
}
