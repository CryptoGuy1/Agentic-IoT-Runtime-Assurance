//! Hidden experimental world and scorer. Neither is imported by the estimator/checker.
use super::{estimate::predict, interval::p, *};
use crate::simulation::{Link, Network, NetworkConfig, sim_json_quote as quote};
use std::time::Duration;

pub const SCENARIOS: &[&str] = &[
    "normal",
    "short-gap",
    "long-gap",
    "delayed-reading",
    "delayed-command",
    "lost-command",
    "occupancy-entry",
    "protected-room",
    "fan-failure",
    "door-jam",
    "conflict",
    "mismatch",
    "pressure-limit",
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrueState {
    pub at: Duration,
    pub hazard: f64,
    pub pressure: f64,
    pub fan: f64,
    pub door: f64,
    pub occupied: bool,
    pub alarm: bool,
    pub fan_failed: bool,
    pub door_jammed: bool,
    pub response_required: bool,
}
impl TrueState {
    pub(crate) fn nominal() -> Self {
        Self {
            at: Duration::ZERO,
            hazard: 0.7,
            pressure: 0.0,
            fan: 0.0,
            door: 0.0,
            occupied: false,
            alarm: false,
            fan_failed: false,
            door_jammed: false,
            response_required: true,
        }
    }
    pub(crate) fn value(&self, variable: Variable) -> Reading {
        match variable {
            Variable::Hazard => Reading::Number(self.hazard),
            Variable::Pressure => Reading::Number(self.pressure),
            Variable::Fan => Reading::Number(self.fan),
            Variable::Door => Reading::Number(self.door),
            Variable::Occupancy => Reading::Boolean(self.occupied),
            Variable::Alarm => Reading::Boolean(self.alarm),
        }
    }
    pub(crate) fn contained_by(&self, s: &StateBox) -> bool {
        self.at == s.at
            && s.hazard.contains(self.hazard)
            && s.pressure.contains(self.pressure)
            && s.fan.contains(self.fan)
            && s.door.contains(self.door)
            && s.occupied.contains(self.occupied)
            && s.alarm.contains(self.alarm)
            && s.fan_failed.contains(self.fan_failed)
            && s.door_jammed.contains(self.door_jammed)
            && s.response_required.contains(self.response_required)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TruePlant {
    pub state: TrueState,
    pub dynamics: Dynamics,
    pub inputs: Inputs,
    pub source: f64,
    pub noise_scale: f64,
    last_ids: [u64; 3],
    alarm_due: Option<Duration>,
}
impl TruePlant {
    pub(crate) fn new(state: TrueState, dynamics: Dynamics, inputs: Inputs) -> Self {
        Self {
            state,
            dynamics,
            inputs,
            source: 0.08,
            noise_scale: 1.0,
            last_ids: [0; 3],
            alarm_due: None,
        }
    }
    pub(crate) fn apply(&mut self, command: &Command) -> bool {
        let device = command.action.device() as usize;
        if command.id <= self.last_ids[device] {
            return false;
        }
        self.last_ids[device] = command.id;
        match command.action {
            Action::AlarmOn => self.alarm_due = Some(self.state.at + Duration::from_millis(200)),
            Action::FanStart => self.inputs.fan = true,
            Action::FanStop => self.inputs.fan = false,
            Action::DoorClose => self.inputs.door = 1,
            Action::DoorOpen => self.inputs.door = -1,
        }
        true
    }
    pub(crate) fn step(&mut self, noise: [f64; 4]) {
        let old = self.state.clone();
        let d = &self.dynamics;
        let dt = 0.1;
        let w = noise.map(|v| v * self.noise_scale);
        self.state.hazard = (old.hazard * (1.0 - dt * (d.hazard_decay + d.fan_removal * old.fan))
            + dt * (self.source + w[0]))
            .clamp(0.0, 1.0);
        self.state.fan = if old.fan_failed {
            (old.fan - dt / 0.8).max(0.0)
        } else {
            (old.fan * (1.0 - dt / d.fan_tau)
                + f64::from(self.inputs.fan) * (dt / d.fan_tau)
                + dt * w[1])
                .clamp(0.0, 1.0)
        };
        self.state.pressure = (old.pressure * (1.0 - dt * d.pressure_decay)
            + dt * (d.pressure_coupling * old.fan * old.door + w[2]))
            .clamp(-1.0, 1.0);
        self.state.door = if old.door_jammed {
            old.door
        } else {
            (old.door + dt * (d.door_speed * f64::from(self.inputs.door) + w[3])).clamp(0.0, 1.0)
        };
        self.state.at += TICK;
        if self.alarm_due.is_some_and(|t| t <= self.state.at) {
            self.state.alarm = true;
            self.inputs.alarm = true;
            self.alarm_due = None;
        }
        if self.state.hazard >= 0.60 {
            self.state.response_required = true;
        } else if self.state.hazard <= 0.30 {
            self.state.response_required = false;
        }
    }
    pub(crate) fn sample(&self, variable: Variable, version: u64, seed: u64) -> Observation {
        let at = self.state.at;
        let value = match self.state.value(variable) {
            Reading::Number(v) => Reading::Number(
                (v + signed(
                    seed,
                    &format!("measurement:{variable:?}:{}", at.as_millis()),
                ) * variable.error().unwrap())
                .clamp(
                    if variable == Variable::Pressure {
                        -1.0
                    } else {
                        0.0
                    },
                    1.0,
                ),
            ),
            value => value,
        };
        Observation {
            id: format!("{variable:?}-{version}"),
            source: format!("sensor-{variable:?}"),
            variable,
            value,
            error: variable.error(),
            generated_at: at,
            received_at: at,
            max_age: variable.max_age(),
            version,
            quality: Quality::Trusted,
        }
    }
}

pub(crate) fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn source_id() -> String {
    let source = [
        include_str!("mod.rs"),
        include_str!("types.rs"),
        include_str!("interval.rs"),
        include_str!("estimate.rs"),
        include_str!("check.rs"),
        include_str!("benchmark.rs"),
        include_str!("../simulation/network.rs"),
        include_str!("../simulation/report.rs"),
        include_str!("../../Cargo.lock"),
    ]
    .join("\n");
    format!("fnv1a64:{:016x}", hash(source.as_bytes()))
}
fn signed(seed: u64, key: &str) -> f64 {
    ((hash(format!("{seed}:{key}").as_bytes()) % 2001) as f64 - 1000.0) / 1000.0
}
pub(crate) fn noise(seed: u64, at: Duration) -> [f64; 4] {
    [0.01, 0.02, 0.02, 0.01]
        .map(|bound| bound * signed(seed, &format!("disturbance:{bound}:{}", at.as_millis())))
}

#[derive(Clone, Debug)]
struct Scenario {
    name: String,
    prior: StateBox,
    actual: TrueState,
    assumptions: Assumptions,
    true_dynamics: Dynamics,
    inputs: Inputs,
    network: NetworkConfig,
    action: Action,
    check_at: Duration,
    end: Duration,
}
impl Scenario {
    fn new(name: &str) -> Result<Self, String> {
        if !SCENARIOS.contains(&name) {
            return Err(format!("unknown scenario; choose {}", SCENARIOS.join(", ")));
        }
        // Declared prior, constructed independently from the hidden actual-state object.
        let prior = StateBox {
            at: Duration::ZERO,
            hazard: Interval::new(0.65, 0.75).unwrap(),
            pressure: Interval::new(-0.05, 0.05).unwrap(),
            fan: p(0.0),
            door: p(0.0),
            occupied: BoolSet::False,
            alarm: BoolSet::False,
            fan_failed: BoolSet::False,
            door_jammed: BoolSet::False,
            response_required: BoolSet::True,
        };
        let mut s = Self {
            name: name.into(),
            prior,
            actual: TrueState::nominal(),
            assumptions: Assumptions::default(),
            true_dynamics: Dynamics::default(),
            inputs: Inputs::default(),
            network: NetworkConfig::default(),
            action: Action::FanStart,
            check_at: Duration::from_secs(2),
            end: Duration::from_secs(10),
        };
        match name {
            "short-gap" => {
                s.check_at = Duration::from_secs(3);
            }
            "long-gap" => {
                s.check_at = Duration::from_secs(4);
                s.prior.pressure = Interval::new(0.65, 0.75).unwrap();
                s.actual.pressure = 0.7;
                s.prior.fan = p(1.0);
                s.actual.fan = 1.0;
                s.inputs.fan = true;
                s.prior.door = p(1.0);
                s.actual.door = 1.0;
            }
            "delayed-reading" => s.network.delay_ms = 400,
            "delayed-command" => s.network.actuator_delay_ms = 600,
            "lost-command" => {}
            "occupancy-entry" => {
                s.action = Action::DoorClose;
                s.check_at = Duration::from_millis(500);
            }
            "protected-room" => {
                s.action = Action::DoorClose;
                s.assumptions.occupancy_stable_until = Duration::from_secs(20);
            }
            "fan-failure" => {
                s.check_at = Duration::from_secs(3);
            }
            "door-jam" => {
                s.action = Action::DoorClose;
                s.assumptions.occupancy_stable_until = Duration::from_secs(20);
                s.check_at = Duration::from_secs(3);
            }
            "conflict" => {}
            "mismatch" => {
                s.true_dynamics.fan_tau = 0.5;
                s.assumptions.faults_may_start = false;
            }
            "pressure-limit" => {
                s.prior.fan = p(1.0);
                s.actual.fan = 1.0;
                s.inputs.fan = true;
                s.prior.door = p(1.0);
                s.actual.door = 1.0;
                s.action = Action::AlarmOn;
                s.check_at = Duration::ZERO;
            }
            _ => {}
        }
        Ok(s)
    }
    fn event(&self, plant: &mut TruePlant) {
        let at = plant.state.at;
        match self.name.as_str() {
            "occupancy-entry" if at >= Duration::from_millis(600) => plant.state.occupied = true,
            "fan-failure" if at >= Duration::from_secs(2) => plant.state.fan_failed = true,
            "door-jam" if at >= Duration::from_secs(2) => plant.state.door_jammed = true,
            _ => {}
        }
    }
    fn drop_sensor(&self, variable: Variable, at: Duration) -> bool {
        match self.name.as_str() {
            "short-gap" => {
                variable == Variable::Pressure
                    && (Duration::from_secs(1)..Duration::from_secs(4)).contains(&at)
            }
            "long-gap" => variable == Variable::Pressure && at >= Duration::from_secs(1),
            "mismatch" => variable == Variable::Fan && at > Duration::ZERO,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OracleResult {
    pub constraint_safe: bool,
    pub physical_safe: bool,
    pub useful: bool,
    pub physical_success: bool,
    pub deadline_met: bool,
    pub initial_violation: bool,
}
/// Actual-state counterfactual, independently checking scalar invariants. It
/// deliberately shares only the defined true plant, never interval/check logic.
fn score(plant: &TruePlant, scenario: &Scenario, seed: u64, action: Action) -> OracleResult {
    let initial_violation = plant.state.pressure.abs() > 0.80;
    let assessed =
        super::oracle::GroundTruthOracle::evaluate(plant, seed, action, |p| scenario.event(p));
    OracleResult {
        constraint_safe: assessed.constraint_safe,
        physical_safe: assessed.physically_safe,
        useful: assessed.useful,
        physical_success: assessed.counterfactual_target,
        deadline_met: assessed.counterfactual_deadline_met,
        initial_violation,
    }
}

/// Exports contain truth for benchmark review only, never policy inputs.
#[derive(Clone, Debug)]
pub struct ExperimentReport {
    pub json: String,
    pub markdown: String,
    pub covered_ticks: usize,
    pub total_ticks: usize,
    pub inconsistent_ticks: usize,
    pub decision: Decision,
}

pub(crate) fn interval_json(i: Interval) -> String {
    format!("[{},{}]", i.lo(), i.hi())
}
fn set_json(s: BoolSet) -> String {
    format!(
        "[{}]",
        s.values()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}
pub(crate) fn box_json(s: &StateBox) -> String {
    format!(
        "{{\"time_ms\":{},\"hazard\":{},\"pressure\":{},\"fan\":{},\"door\":{},\"occupied\":{},\"alarm\":{},\"fan_failed\":{},\"door_jammed\":{},\"response_required\":{}}}",
        s.at.as_millis(),
        interval_json(s.hazard),
        interval_json(s.pressure),
        interval_json(s.fan),
        interval_json(s.door),
        set_json(s.occupied),
        set_json(s.alarm),
        set_json(s.fan_failed),
        set_json(s.door_jammed),
        set_json(s.response_required)
    )
}
pub(crate) fn truth_json(s: &TrueState) -> String {
    format!(
        "{{\"time_ms\":{},\"hazard\":{},\"pressure\":{},\"fan\":{},\"door\":{},\"occupied\":{},\"alarm\":{},\"fan_failed\":{},\"door_jammed\":{},\"response_required\":{}}}",
        s.at.as_millis(),
        s.hazard,
        s.pressure,
        s.fan,
        s.door,
        s.occupied,
        s.alarm,
        s.fan_failed,
        s.door_jammed,
        s.response_required
    )
}
pub(crate) fn check_json(c: &CheckResult) -> String {
    format!(
        "{{\"decision\":{},\"action\":{},\"checked_at_ms\":{},\"horizon_end_ms\":{},\"reasons\":[{}],\"observation_ids\":[{}],\"pressure_envelope\":{}}}",
        quote(&format!("{:?}", c.decision)),
        quote(&format!("{:?}", c.action)),
        c.checked_at.as_millis(),
        c.horizon_end.as_millis(),
        c.reasons
            .iter()
            .map(|r| quote(r))
            .collect::<Vec<_>>()
            .join(","),
        c.observations
            .iter()
            .map(|r| quote(r))
            .collect::<Vec<_>>()
            .join(","),
        interval_json(c.pressure_envelope)
    )
}
pub(crate) fn observation_json(o: &Observation, update: Update) -> String {
    let value = match o.value {
        Reading::Number(v) => v.to_string(),
        Reading::Boolean(v) => v.to_string(),
    };
    format!(
        "{{\"id\":{},\"source\":{},\"variable\":{},\"value\":{},\"error_bound\":{},\"generated_ms\":{},\"received_ms\":{},\"max_age_ms\":{},\"version\":{},\"quality\":{},\"update\":{}}}",
        quote(&o.id),
        quote(&o.source),
        quote(&format!("{:?}", o.variable)),
        value,
        o.error.map_or("null".into(), |v| v.to_string()),
        o.generated_at.as_millis(),
        o.received_at.as_millis(),
        o.max_age.as_millis(),
        o.version,
        quote(&format!("{:?}", o.quality)),
        quote(&format!("{update:?}"))
    )
}

pub(crate) fn dynamics_json(d: &Dynamics) -> String {
    format!(
        "{{\"hazard_decay\":{},\"fan_removal\":{},\"fan_tau\":{},\"pressure_decay\":{},\"pressure_coupling\":{},\"door_speed\":{}}}",
        d.hazard_decay,
        d.fan_removal,
        d.fan_tau,
        d.pressure_decay,
        d.pressure_coupling,
        d.door_speed
    )
}
fn configuration_json(s: &Scenario) -> String {
    format!(
        "{{\"initial_prior\":{},\"initial_actual\":{},\"estimator_dynamics\":{},\"true_dynamics\":{},\"source_bounds\":{},\"true_source\":0.08,\"disturbance_scale\":{},\"true_disturbance_scale\":1,\"faults_may_start\":{},\"occupancy_stable_until_ms\":{},\"network_encoding\":{},\"initial_inputs\":{{\"fan\":{},\"door\":{},\"alarm\":{}}},\"check_at_ms\":{},\"end_ms\":{},\"event_definition\":{},\"contract_version\":\"component-constraints-v1\",\"pressure_limit\":0.8,\"fan_success\":0.9,\"fan_stopped\":0.05}}",
        box_json(&s.prior),
        truth_json(&s.actual),
        dynamics_json(&s.assumptions.dynamics),
        dynamics_json(&s.true_dynamics),
        interval_json(s.assumptions.source),
        s.assumptions.disturbance_scale,
        s.assumptions.faults_may_start,
        s.assumptions.occupancy_stable_until.as_millis(),
        quote(&s.network.encode()),
        s.inputs.fan,
        s.inputs.door,
        s.inputs.alarm,
        s.check_at.as_millis(),
        s.end.as_millis(),
        quote(&format!("{MODEL_VERSION}/{}", s.name))
    )
}

pub fn run_experiment(name: &str, seed: u64) -> Result<ExperimentReport, String> {
    let scenario = Scenario::new(name)?;
    let config = format!("{MODEL_VERSION}|{seed}|{scenario:?}");
    let config_id = format!("fnv1a64:{:016x}", hash(config.as_bytes()));
    let mut plant = TruePlant::new(
        scenario.actual.clone(),
        scenario.true_dynamics.clone(),
        scenario.inputs,
    );
    let mut estimator = Estimator::new(
        scenario.prior.clone(),
        scenario.inputs,
        scenario.assumptions.clone(),
    )?;
    let mut net = Network::new(scenario.network.clone(), seed)?;
    let mut versions = [0u64; 6];
    let mut pending_obs: Vec<Observation> = vec![];
    let mut pending_commands: Vec<(Duration, Command)> = vec![];
    let mut commands = vec![];
    let mut observations = vec![];
    let mut rows = vec![];
    let mut covered = 0;
    let mut inconsistent = 0;
    let mut captured = None;
    let mut command_events = vec![];
    loop {
        let now = plant.state.at;
        scenario.event(&mut plant);
        // Scripted interventions exercise the physical model, not a closed-loop policy.
        if now == Duration::ZERO {
            let action = if name == "door-jam" {
                Action::DoorClose
            } else if name == "protected-room" {
                Action::AlarmOn
            } else {
                Action::FanStart
            };
            let c = Command {
                id: 1,
                action,
                issued_at: now,
                max_delay: Duration::from_millis(
                    (scenario.network.delay_ms
                        + scenario.network.actuator_delay_ms
                        + scenario.network.jitter_ms
                        + scenario.network.reorder_ms)
                        .div_ceil(100)
                        * 100,
                ),
                may_drop: name == "lost-command",
            };
            estimator.note_command(c.clone())?;
            commands.push(c.clone());
            if name != "lost-command" {
                for at in net.send(Link::Actuator, "scripted-command-1", now) {
                    pending_commands.push((at, c.clone()));
                }
            }
            command_events.push(format!("{{\"id\":1,\"action\":{},\"kind\":\"issued\",\"at_ms\":0,\"max_delay_ms\":{},\"may_drop\":{}}}",quote(&format!("{action:?}")),c.max_delay.as_millis(),c.may_drop));
        }
        pending_commands.sort_by_key(|(at, c)| (*at, c.id));
        let mut retained = vec![];
        for (at, c) in pending_commands.drain(..) {
            if at <= now {
                let applied = plant.apply(&c);
                command_events.push(format!(
                    "{{\"id\":{},\"kind\":{},\"received_ms\":{},\"applied_tick_ms\":{}}}",
                    c.id,
                    quote(if applied {
                        "applied"
                    } else {
                        "duplicate_or_obsolete"
                    }),
                    at.as_millis(),
                    now.as_millis()
                ));
            } else {
                retained.push((at, c));
            }
        }
        pending_commands = retained;
        for (i, variable) in Variable::ALL.into_iter().enumerate() {
            if now.as_nanos().is_multiple_of(variable.period().as_nanos()) {
                versions[i] += 1;
                let obs = plant.sample(variable, versions[i], seed);
                if !scenario.drop_sensor(variable, now) {
                    for at in net.send(Link::Sensor, &obs.id, now) {
                        let mut delivered = obs.clone();
                        delivered.received_at = at;
                        pending_obs.push(delivered);
                    }
                }
            }
        }
        pending_obs.sort_by(|a, b| (a.received_at, &a.id).cmp(&(b.received_at, &b.id)));
        let mut retained = vec![];
        for obs in pending_obs.drain(..) {
            if obs.received_at <= now {
                let update = estimator.observe(obs.clone())?;
                observations.push(observation_json(&obs, update));
            } else {
                retained.push(obs);
            }
        }
        pending_obs = retained;
        if name == "conflict" && now == Duration::from_secs(1) {
            let obs = Observation {
                id: "conflicting-alarm".into(),
                source: "second-alarm-sensor".into(),
                variable: Variable::Alarm,
                value: Reading::Boolean(true),
                error: None,
                generated_at: now,
                received_at: now,
                max_age: Duration::from_secs(1),
                version: 1,
                quality: Quality::Trusted,
            };
            let update = estimator.observe(obs.clone())?;
            observations.push(observation_json(&obs, update));
        }
        let contains = plant.state.contained_by(estimator.state());
        covered += usize::from(contains);
        inconsistent += usize::from(estimator.inconsistent());
        rows.push(format!(
            "{{\"actual\":{},\"estimate\":{},\"contained\":{},\"inconsistent\":{}}}",
            truth_json(&plant.state),
            box_json(estimator.state()),
            contains,
            estimator.inconsistent()
        ));
        if now == scenario.check_at {
            let checked = estimator.check(scenario.action);
            // Freshness diagnostic retains the same model check, but adds the direct-age requirement.
            let needed: &[Variable] = match scenario.action {
                Action::DoorClose => &[Variable::Pressure, Variable::Occupancy],
                Action::FanStop => &[Variable::Pressure, Variable::Hazard],
                _ => &[Variable::Pressure],
            };
            let fresh = needed.iter().all(|v| {
                estimator
                    .observations()
                    .iter()
                    .any(|o| o.variable == *v && now < o.generated_at + o.max_age)
            });
            let freshness = if fresh {
                checked.decision
            } else {
                Decision::Unknown
            };
            // Diagnostic point projection of the same trajectory, deliberately
            // collapsing each step; it is not presented as a complete estimator baseline.
            let mut point = estimator.state().point_projection();
            let mut point_commands = commands.clone();
            point_commands.push(Command {
                id: 2,
                action: scenario.action,
                issued_at: now,
                max_delay: Duration::ZERO,
                may_drop: false,
            });
            let mut point_safe = true;
            loop {
                point_safe &= point.pressure.midpoint().abs() <= 0.80
                    && (scenario.action != Action::DoorClose || point.occupied == BoolSet::False)
                    && (scenario.action != Action::FanStop
                        || point.response_required == BoolSet::False);
                if point.at >= now + scenario.action.horizon() {
                    break;
                }
                point = predict(
                    &point,
                    &scenario.assumptions,
                    scenario.inputs,
                    &point_commands,
                )
                .point_projection();
            }
            let point_decision = if estimator.inconsistent() {
                Decision::Unknown
            } else if point_safe {
                Decision::Supported
            } else {
                Decision::Contradicted
            };
            let oracle = score(&plant, &scenario, seed, scenario.action);
            captured = Some((checked, freshness, point_decision, oracle, contains));
        }
        if now == scenario.end {
            break;
        }
        plant.step(noise(seed, now));
        estimator.advance_to(plant.state.at)?;
    }
    let (checked, freshness, point_decision, oracle, decision_coverage) =
        captured.ok_or("missing decision fixture")?;
    let total = rows.len();
    let json = format!(
        "{{\"schema_version\":1,\"model_version\":{},\"source_id\":{},\"experiment_kind\":\"component_diagnostic\",\"scenario\":{},\"seed\":{},\"config_id\":{},\"configuration\":{},\"tick_ms\":100,\"candidate_application\":\"counterfactual_immediate\",\"range_check\":{},\"freshness_plus_model\":{},\"optimistic_point_projection\":{},\"oracle\":{{\"constraint_safe\":{},\"physical_safe\":{},\"useful\":{},\"physical_success\":{},\"deadline_met\":{},\"initial_violation\":{},\"outcome_confirmed\":false}},\"coverage\":{{\"contained_ticks\":{},\"total_ticks\":{},\"decision_state_contained\":{},\"inconsistent_ticks\":{}}},\"commands\":[{}],\"delivered_observations\":[{}],\"trace\":[{}]}}\n",
        quote(MODEL_VERSION),
        quote(&source_id()),
        quote(name),
        seed,
        quote(&config_id),
        configuration_json(&scenario),
        check_json(&checked),
        quote(&format!("{freshness:?}")),
        quote(&format!("{point_decision:?}")),
        oracle.constraint_safe,
        oracle.physical_safe,
        oracle.useful,
        oracle.physical_success,
        oracle.deadline_met,
        oracle.initial_violation,
        covered,
        total,
        decision_coverage,
        inconsistent,
        command_events.join(","),
        observations.join(","),
        rows.join(",")
    );
    let markdown = format!(
        "# Model v1 component experiment: {name}\n\nModel: `{MODEL_VERSION}` · Seed: {seed} · Configuration: `{config_id}`\n\nAt {} ms, the proposed action is **{:?}**.\n\n| Component check | Decision |\n| --- | --- |\n| Range-based model | {:?} |\n| Fresh readings plus model | {:?} |\n| Optimistic point projection | {:?} |\n\n{}\n\nThe actual state was inside the estimated possibilities on **{covered}/{total}** ticks. At the decision it was **{decision_coverage}**. Inconsistent ticks: **{inconsistent}**.\n\nThe independent counterfactual scorer reports: configured constraints satisfied **{}**; physically safe **{}**; useful **{}**; achieved physical target **{}**; deadline met **{}**. A counterfactual physical result is not confirmed device feedback.\n\nThis is a scripted component diagnostic. The candidate is evaluated as applied immediately, with prior command uncertainty retained by the estimator. No B0–B6 policy ranking, execution authorization, indefinite safety or 80% correctness claim is made. The mismatch fixture deliberately violates model assumptions.\n\nReplay with the same source version:\n\n```sh\ncargo run --offline --bin model_v1 -- --scenario {name} --seed {seed} --out NEW_DIRECTORY\n```\n",
        checked.checked_at.as_millis(),
        checked.action,
        checked.decision,
        freshness,
        point_decision,
        checked.reasons.join(" "),
        oracle.constraint_safe,
        oracle.physical_safe,
        oracle.useful,
        oracle.physical_success,
        oracle.deadline_met
    );
    Ok(ExperimentReport {
        json,
        markdown,
        covered_ticks: covered,
        total_ticks: total,
        inconsistent_ticks: inconsistent,
        decision: checked.decision,
    })
}

#[cfg(test)]
pub(crate) fn test_score(plant: &TruePlant, action: Action) -> OracleResult {
    score(plant, &Scenario::new("normal").unwrap(), 42, action)
}
