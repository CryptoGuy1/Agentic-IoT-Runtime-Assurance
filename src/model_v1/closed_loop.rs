//! Experiment owner: hidden plant -> messages -> runtime -> devices -> feedback.
//! Only the adapter sees truth during execution. Scoring happens after the run.
use super::{
    benchmark::{self, TruePlant, TrueState},
    execution_score::{self, CompletionClaim, ExecutionBoundary},
    *,
};
use crate::actions::*;
use crate::evidence::{AssuranceStatus as Status, EvidenceAtom};
use crate::graph::{AssuranceGraph, Justification, Node, NodeKind};
use crate::simulation::sim_json_quote as quote;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

pub const CLOSED_LOOP_SCHEMA: &str = "model-closed-loop-v1";
pub const CLOSED_LOOP_SCENARIOS: &[&str] = &[
    "normal",
    "short-gap",
    "recovery",
    "uncertain",
    "lost-feedback",
    "delayed-command",
    "delayed-reading",
    "hazard-recovery",
    "replanning",
];
#[derive(Clone, Copy, Debug)]
pub struct RunOptions {
    pub policy: RefinementPolicy,
    pub autonomous: bool,
    pub broader_recovery: bool,
    pub benchmark: bool,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            policy: RefinementPolicy::B6,
            autonomous: false,
            broader_recovery: false,
            benchmark: false,
        }
    }
}
fn ms(t: u64) -> Duration {
    Duration::from_millis(t)
}
fn p(v: f64) -> Interval {
    Interval::point(v).unwrap()
}
fn error(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
fn hash(text: &str) -> String {
    format!("fnv1a64:{:016x}", benchmark::hash(text.as_bytes()))
}
fn source_id() -> String {
    hash(
        &[
            include_str!("closed_loop.rs"),
            include_str!("policies.rs"),
            include_str!("planning.rs"),
            include_str!("execution_score.rs"),
            include_str!("benchmark.rs"),
            include_str!("integration.rs"),
            include_str!("claims.rs"),
            include_str!("estimate.rs"),
            include_str!("check.rs"),
            include_str!("types.rs"),
            include_str!("interval.rs"),
            include_str!("mod.rs"),
            include_str!("../actions/execution.rs"),
            include_str!("../actions/types.rs"),
            include_str!("../runtime.rs"),
            include_str!("../evidence.rs"),
            include_str!("../graph.rs"),
            include_str!("../evaluator.rs"),
            include_str!("../incremental.rs"),
            include_str!("../simulation/report.rs"),
            include_str!("../clock.rs"),
            include_str!("../../Cargo.lock"),
        ]
        .join("\n"),
    )
}
#[derive(Clone, Debug)]
struct Configuration {
    name: String,
    prior: StateBox,
    actual: TrueState,
    assumptions: Assumptions,
    true_dynamics: Dynamics,
    propose_at: Duration,
    command_delay: Duration,
    sensor_delay: Duration,
    feedback_delay: Duration,
    end: Duration,
}
impl Configuration {
    fn new(name: &str) -> Result<Self, String> {
        if !CLOSED_LOOP_SCENARIOS.contains(&name) {
            return Err(format!(
                "unknown closed-loop scenario; choose {}",
                CLOSED_LOOP_SCENARIOS.join(", ")
            ));
        }
        // Public fixture prior, declared independently of the hidden state below.
        let prior = StateBox {
            at: Duration::ZERO,
            hazard: p(0.0),
            pressure: Interval::new(-0.05, 0.05)?,
            fan: p(0.0),
            door: p(if name == "hazard-recovery" {
                0.85
            } else if name == "recovery" || name == "replanning" {
                0.9
            } else {
                0.0
            }),
            occupied: BoolSet::False,
            alarm: BoolSet::False,
            fan_failed: BoolSet::False,
            door_jammed: BoolSet::False,
            response_required: BoolSet::False,
        };
        let actual = TrueState {
            at: Duration::ZERO,
            hazard: 0.0,
            pressure: 0.0,
            fan: 0.0,
            door: if name == "hazard-recovery" {
                0.85
            } else if name == "recovery" || name == "replanning" {
                0.9
            } else {
                0.0
            },
            occupied: false,
            alarm: false,
            fan_failed: false,
            door_jammed: false,
            response_required: false,
        };
        let mut config = Self {
            name: name.into(),
            prior,
            actual,
            assumptions: Assumptions {
                source: p(0.0),
                occupancy_stable_until: ms(20000),
                ..Assumptions::default()
            },
            true_dynamics: Dynamics::default(),
            propose_at: ms(if name == "short-gap" { 1000 } else { 500 }),
            command_delay: ms(if name == "delayed-command" { 600 } else { 100 }),
            sensor_delay: ms(if name == "delayed-reading" { 300 } else { 0 }),
            feedback_delay: ms(100),
            end: ms(8000),
        };
        if name == "uncertain" {
            config.prior.pressure = Interval::new(0.76, 0.82)?;
            config.actual.pressure = 0.79;
            config.propose_at = Duration::ZERO;
        }
        if name == "replanning" {
            config.assumptions.faults_may_start = false;
        }
        if name == "hazard-recovery" {
            config.prior.hazard = Interval::new(0.65, 0.75)?;
            config.actual.hazard = 0.70;
            config.prior.response_required = BoolSet::True;
            config.actual.response_required = true;
            config.assumptions.faults_may_start = false;
        }
        Ok(config)
    }
    fn drop_sensor(&self, variable: Variable, now: Duration) -> bool {
        self.name == "short-gap"
            && variable == Variable::Pressure
            && (ms(500)..ms(4000)).contains(&now)
    }
}
fn setup(config: &Configuration, options: RunOptions) -> Result<ModelRuntime, String> {
    let mut nodes: Vec<_> = ["permission", "pressure.direct", "fan.ack", "fan.effect"]
        .iter()
        .chain(["global.fresh"].iter())
        .map(|id| Node::new(*id, NodeKind::Evidence))
        .collect();
    nodes.extend([
        Node::new("fan.start", NodeKind::ActionStart),
        Node::new("fan.run", NodeKind::ActionRun),
        Node::new("fan.outcome", NodeKind::ActionOutcome),
    ]);
    let rule = |id: &str, premises: &[&str]| Justification {
        justification_id: format!("{id}.rule"),
        premises: premises.iter().map(|p| (*p).into()).collect(),
        threshold: premises.len(),
        conclusion: id.into(),
    };
    let start_premises: &[&str] = match options.policy {
        RefinementPolicy::B0 | RefinementPolicy::B3 => &["permission"],
        RefinementPolicy::B4 => &["permission", "pressure.direct", "global.fresh"],
        _ => &["permission", "pressure.direct"],
    };
    let run_premises: &[&str] = match options.policy {
        RefinementPolicy::B0 | RefinementPolicy::B2 | RefinementPolicy::B3 => &["permission"],
        RefinementPolicy::B4 => &["permission", "pressure.direct", "global.fresh"],
        _ => &["permission", "pressure.direct"],
    };
    let graph = AssuranceGraph::new(
        nodes,
        vec![
            rule("fan.start", start_premises),
            rule("fan.run", run_premises),
            rule("fan.outcome", &["fan.ack", "fan.effect"]),
        ],
    )
    .map_err(error)?;
    let definitions = vec![ActionDefinition {
        action_id: "fan".into(),
        start_contract: "fan.start".into(),
        run_contract: Some("fan.run".into()),
        commit_contract: None,
        outcome_contract: "fan.outcome".into(),
        min_duration: ms(200),
        max_duration: ms(10000),
        interruptibility: Interruptibility::Preemptible,
        recovery_type: RecoveryType::Reversible,
        resources_read: BTreeSet::new(),
        resources_written: BTreeSet::from(["fan".into()]),
        max_start_lease: ms(200),
        controllers: ControllerCapabilities {
            safe_abort: true,
            ..ControllerCapabilities::default()
        },
        outcomes: OutcomeBindings {
            acknowledgement: "fan.ack".into(),
            observed: BTreeMap::from([("target".into(), "fan.effect".into())]),
            max_age: ms(2000),
        },
    }];
    let claims = ClaimConfiguration {
        revision: "closed-loop-contracts-v1".into(),
        replacements: vec![PhysicalClaimBinding {
            direct_evidence: "pressure.direct".into(),
            model_evidence: "pressure.model".into(),
            predicate: PhysicalPredicate::PressureWithinLimit,
        }],
        protected_evidence: BTreeSet::from(["permission".into()]),
    };
    let mut nodes: Vec<_> = graph
        .nodes()
        .iter()
        .map(|(id, k)| Node::new(id, *k))
        .collect();
    let mut rules: Vec<_> = graph.justifications().values().cloned().collect();
    let mut definitions = definitions;
    let mut bindings = BTreeMap::from([(
        "fan".into(),
        ModelBinding {
            action: Action::FanStart,
            recovery: Action::FanStop,
            max_delay: config.command_delay,
            may_drop: false,
        },
    )]);
    if options.autonomous || options.broader_recovery {
        for id in ["open.ack", "open.effect"] {
            nodes.push(Node::new(id, NodeKind::Evidence));
        }
        for (id, kind) in [
            ("open.start", NodeKind::ActionStart),
            ("open.run", NodeKind::ActionRun),
            ("open.outcome", NodeKind::ActionOutcome),
        ] {
            nodes.push(Node::new(id, kind));
        }
        rules.extend([
            rule("open.start", start_premises),
            rule("open.run", run_premises),
            rule("open.outcome", &["open.ack", "open.effect"]),
        ]);
        let mut open = definitions[0].clone();
        open.action_id = "open".into();
        open.start_contract = "open.start".into();
        open.run_contract = Some("open.run".into());
        open.outcome_contract = "open.outcome".into();
        open.resources_written = BTreeSet::from(["door".into()]);
        open.outcomes.acknowledgement = "open.ack".into();
        open.outcomes.observed = BTreeMap::from([("target".into(), "open.effect".into())]);
        definitions.push(open);
        bindings.insert(
            "open".into(),
            ModelBinding {
                action: Action::DoorOpen,
                recovery: Action::DoorOpen,
                max_delay: config.command_delay,
                may_drop: false,
            },
        );
    }
    let graph = AssuranceGraph::new(nodes, rules).map_err(error)?;
    let graph = if options.policy == RefinementPolicy::B6 {
        substitute_physical_claims(&graph, &definitions, &claims)?
    } else {
        graph
    };
    let runtime = ActionRuntime::new(graph, definitions).map_err(error)?;
    let estimator = Estimator::new(
        config.prior.clone(),
        Inputs::default(),
        config.assumptions.clone(),
    )?;
    let mut runtime = if options.policy == RefinementPolicy::B6 {
        ModelRuntime::with_claims(runtime, estimator, bindings, claims)
    } else {
        ModelRuntime::new(runtime, estimator, bindings)
    }
    .map_err(error)?;
    runtime.comparison_policy(options.policy);
    if options.broader_recovery {
        runtime
            .configure_recoveries("fan", vec![Action::DoorOpen])
            .map_err(error)?;
    }
    runtime
        .update_permission(EvidenceAtom {
            evidence_id: "permission".into(),
            evidence_type: "permission".into(),
            predicate: "fan-test-authorized".into(),
            source_id: "fixture-operator".into(),
            version: 1,
            observed_at: Duration::ZERO,
            expires_at: ms(20000),
            status: Status::Valid,
            payload_hash: None,
        })
        .map_err(error)?;
    runtime
        .register_plan(Plan {
            plan_id: "fan-test".into(),
            version: 1,
            actions: BTreeMap::from([("fan-1".into(), "fan".into())]),
            precedence: BTreeSet::new(),
            created_at: Duration::ZERO,
        })
        .map_err(error)?;
    Ok(runtime)
}
#[derive(Clone, Debug)]
struct DeviceExecution {
    command: PhysicalCommand,
    applied_at: Duration,
    completion_sent: bool,
}
#[derive(Clone, Debug)]
struct Feedback {
    at: Duration,
    physical_id: u64,
    result: ActuatorResult,
}
/// Device-side exact simulated feedback sensor. The policy receives only the
/// correlated report; this does not read a model verdict or the independent scorer.
fn measured_target(plant: &TruePlant, action: Action) -> bool {
    match (
        action,
        plant.state.value(match action {
            Action::AlarmOn => Variable::Alarm,
            Action::FanStart | Action::FanStop => Variable::Fan,
            _ => Variable::Door,
        }),
    ) {
        (Action::AlarmOn, Reading::Boolean(on)) => on,
        (Action::FanStart, Reading::Number(v)) => v >= 0.90,
        (Action::FanStop, Reading::Number(v)) => v <= 0.05,
        (Action::DoorClose, Reading::Number(v)) => v >= 0.95,
        (Action::DoorOpen, Reading::Number(v)) => v <= 0.05,
        _ => false,
    }
}
fn packet(
    device: &DeviceExecution,
    now: Duration,
    delay: Duration,
    completed: bool,
) -> Option<Feedback> {
    let command = device.command.runtime_command.as_ref();
    let normal = command.is_some_and(|c| {
        matches!(
            c.kind,
            CommandKind::Start | CommandKind::Commit | CommandKind::CertifiedComplete
        )
    });
    Some(Feedback {
        at: now + delay,
        physical_id: device.command.model_command.id,
        result: ActuatorResult {
            command_id: command.map_or_else(
                || format!("physical-{}", device.command.model_command.id),
                |c| c.command_id.clone(),
            ),
            sequence: if completed { 2 } else { 1 },
            acknowledged: Some(true),
            observed_state: if normal && completed {
                BTreeMap::from([("target".into(), Status::Valid)])
            } else {
                BTreeMap::new()
            },
            completed,
            observed_at: now,
        },
    })
}
fn event(now: Duration, kind: &str, details: String) -> String {
    format!(
        "{{\"at_ms\":{},\"kind\":{},\"details\":{details}}}",
        now.as_millis(),
        quote(kind)
    )
}
fn maybe_time(at: Option<Duration>) -> String {
    at.map_or("null".into(), |t| t.as_millis().to_string())
}
fn score_json(score: &ExecutionScore) -> String {
    let commands = score.commands.iter().map(|c| format!("{{\"id\":{},\"action\":{},\"released_ms\":{},\"applied_ms\":{},\"target_ms\":{},\"target_by_deadline\":{},\"useful_at_release\":{},\"window_complete\":{},\"physical_safe_in_observed_window\":{},\"constraint_safe_in_observed_window\":{},\"feedback_confirmed_ms\":{}}}", c.id, quote(&format!("{:?}", c.action)), c.released_at.as_millis(), maybe_time(c.applied_at), maybe_time(c.physical_target_at), c.target_by_deadline.map_or("null".into(), |v| v.to_string()), c.useful_at_release, c.window_complete, c.physical_safe_in_window, c.constraint_safe_in_window, maybe_time(c.feedback_confirmed_at))).collect::<Vec<_>>().join(",");
    format!(
        "{{\"pressure_violation_ticks\":{},\"occupied_closing_motion_ticks\":{},\"initial_pressure_violation\":{},\"false_completion_claims\":{},\"physical_targets_without_confirmation\":{},\"objective_achieved\":{},\"commands\":[{commands}]}}",
        score.pressure_violation_ticks,
        score.occupied_closing_motion_ticks,
        score.initial_pressure_violation,
        score.false_completion_claims,
        score.physical_targets_without_confirmation,
        score.objective_achieved
    )
}
fn certificate_json(c: &PhysicalCertificate) -> String {
    let references = c
        .observations
        .iter()
        .map(|o| quote(&o.id))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema\":{},\"model_version\":{},\"configuration_revision\":{},\"evidence_id\":{},\"predicate\":{},\"version\":{},\"state_revision\":{},\"checked_ms\":{},\"valid_until_ms\":{},\"status\":{},\"observation_ids\":[{}],\"command_ids\":[{}],\"affected_contracts\":[{}],\"state\":{},\"reason\":{}}}",
        quote(c.schema),
        quote(c.model_version),
        quote(&c.configuration_revision),
        quote(&c.evidence_id),
        quote(&format!("{:?}", c.predicate)),
        c.version,
        c.state_revision,
        c.checked_at.as_millis(),
        c.valid_until.as_millis(),
        quote(&format!("{:?}", c.status)),
        references,
        c.command_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        c.affected_contracts
            .iter()
            .map(|s| quote(s))
            .collect::<Vec<_>>()
            .join(","),
        benchmark::box_json(&c.state),
        quote(&c.reason)
    )
}
#[derive(Clone, Debug)]
pub struct ClosedLoopReport {
    pub json: String,
    pub markdown: String,
    pub events_jsonl: String,
    pub score: ExecutionScore,
    pub final_state: ActionState,
    pub contained_ticks: usize,
    pub total_ticks: usize,
    pub start_allowed: bool,
}

pub fn run_closed_loop(name: &str, seed: u64) -> Result<ClosedLoopReport, String> {
    run_closed_loop_with_options(name, seed, RunOptions::default())
}
pub fn run_closed_loop_with_options(
    name: &str,
    seed: u64,
    mut options: RunOptions,
) -> Result<ClosedLoopReport, String> {
    if options.policy != RefinementPolicy::B6 {
        options.benchmark = true;
    }
    let mut config = Configuration::new(name)?;
    if options.benchmark {
        config.end = ms(12000);
    }
    if options.autonomous {
        config.end = ms(14000);
    }
    let config_id = hash(&format!(
        "{CLOSED_LOOP_SCHEMA}|{seed}|{config:?}|{options:?}"
    ));
    let mut runtime = setup(&config, options)?;
    let mut plant = TruePlant::new(
        config.actual.clone(),
        config.true_dynamics.clone(),
        Inputs::default(),
    );
    plant.source = 0.0;
    let mut observations = vec![];
    let mut sensor_queue: Vec<Observation> = vec![];
    let mut command_queue: Vec<(Duration, PhysicalCommand)> = vec![];
    let mut devices: Vec<DeviceExecution> = vec![];
    let mut feedback_queue: Vec<Feedback> = vec![];
    let mut boundaries: Vec<ExecutionBoundary> = vec![];
    let mut completions: Vec<CompletionClaim> = vec![];
    let mut truth = vec![];
    let mut events = vec![];
    let mut rows = vec![];
    let mut versions = [0u64; 6];
    let mut contained = 0;
    let mut start_allowed = false;
    let mut proposal_status = String::new();
    let mut certificate_cursor = 0;
    let mut runtime_cursor = 0;
    let mut model_cursor = 0;
    let mut completion_recorded = BTreeSet::new();
    let mut supervisor = GoalSupervisor::default();
    let mut latest: BTreeMap<Variable, Duration> = BTreeMap::new();
    let mut global_version = 0;
    let mut permission_version = 1;
    loop {
        let now = plant.state.at;
        // Physics used previous inputs; receipts affect the following step.
        command_queue.sort_by_key(|(at, c)| (*at, c.model_command.id));
        let due: Vec<_> = command_queue.extract_if(.., |(at, _)| *at <= now).collect();
        for (_, command) in due {
            let applied = plant.apply(&command.model_command);
            events.push(event(
                now,
                "device-receipt",
                format!(
                    "{{\"id\":{},\"applied\":{applied}}}",
                    command.model_command.id
                ),
            ));
            if applied {
                boundaries
                    .iter_mut()
                    .find(|b| b.id == command.model_command.id)
                    .unwrap()
                    .applied_at = Some(now);
                let device = DeviceExecution {
                    command,
                    applied_at: now,
                    completion_sent: false,
                };
                if let Some(feedback) = packet(&device, now, config.feedback_delay, false) {
                    feedback_queue.push(feedback);
                }
                devices.push(device);
            }
        }
        // Regular sensors have bounded error, with zero measurement error in
        // these fixtures. Disturbances remain seeded and within declared bounds.
        for (i, variable) in Variable::ALL.into_iter().enumerate() {
            if now.as_nanos().is_multiple_of(variable.period().as_nanos()) {
                versions[i] += 1;
                let mut obs = plant.sample(variable, versions[i], seed);
                obs.value = plant.state.value(variable);
                if variable == Variable::Pressure {
                    obs.max_age = ms(if options.benchmark { 750 } else { 500 });
                }
                if config.drop_sensor(variable, now) {
                    events.push(event(
                        now,
                        "sensor-message-dropped",
                        format!(
                            "{{\"id\":{},\"variable\":{}}}",
                            quote(&obs.id),
                            quote(&format!("{variable:?}"))
                        ),
                    ));
                } else {
                    obs.received_at = now + config.sensor_delay;
                    sensor_queue.push(obs);
                }
            }
        }
        sensor_queue.sort_by(|a, b| (a.received_at, &a.id).cmp(&(b.received_at, &b.id)));
        let due: Vec<_> = sensor_queue
            .extract_if(.., |o| o.received_at <= now)
            .collect();
        for obs in due {
            let update = runtime.observe(obs.clone()).map_err(error)?;
            if update == Update::Applied {
                latest.insert(obs.variable, obs.generated_at + obs.max_age);
            }
            let serialized = benchmark::observation_json(&obs, update);
            events.push(event(now, "observation-delivered", serialized.clone()));
            observations.push(serialized);
            if obs.variable == Variable::Pressure {
                let Reading::Number(v) = obs.value else {
                    unreachable!()
                };
                runtime
                    .update_permission(EvidenceAtom {
                        evidence_id: "pressure.direct".into(),
                        evidence_type: "direct-sensor-predicate".into(),
                        predicate: "pressure-point-within-limit".into(),
                        source_id: obs.source.clone(),
                        version: obs.version,
                        observed_at: obs.generated_at,
                        expires_at: if options.policy == RefinementPolicy::B1 {
                            ms(20000)
                        } else {
                            obs.generated_at + obs.max_age
                        },
                        status: if v.abs() <= 0.80 {
                            Status::Valid
                        } else {
                            Status::Invalid
                        },
                        payload_hash: Some(obs.id),
                    })
                    .map_err(error)?;
            }
        }
        if options.policy == RefinementPolicy::B4 {
            global_version += 1;
            let all_fresh = Variable::ALL
                .iter()
                .all(|v| latest.get(v).is_some_and(|until| now < *until));
            let expires = latest
                .values()
                .copied()
                .filter(|at| *at > now)
                .min()
                .unwrap_or(now + TICK);
            runtime
                .update_permission(EvidenceAtom {
                    evidence_id: "global.fresh".into(),
                    evidence_type: "global-freshness".into(),
                    predicate: "all-six-channels-fresh".into(),
                    source_id: "delivered-message-adapter".into(),
                    version: global_version,
                    observed_at: now,
                    expires_at: expires,
                    status: if all_fresh {
                        Status::Valid
                    } else {
                        Status::Unknown
                    },
                    payload_hash: None,
                })
                .map_err(error)?;
        }
        if name == "hazard-recovery" && now == ms(2000) {
            permission_version += 1;
            runtime
                .update_permission(EvidenceAtom {
                    evidence_id: "permission".into(),
                    evidence_type: "permission".into(),
                    predicate: "fan-test-authorized".into(),
                    source_id: "fixture-operator".into(),
                    version: permission_version,
                    observed_at: now,
                    expires_at: ms(20000),
                    status: Status::Invalid,
                    payload_hash: None,
                })
                .map_err(error)?;
            events.push(event(now, "operator-permission-revoked", "{}".into()));
        }
        let latest_device_commands: BTreeMap<_, _> = devices
            .iter()
            .map(|d| {
                (
                    d.command.model_command.action.device(),
                    d.command.model_command.id,
                )
            })
            .collect();
        for device in &mut devices {
            if latest_device_commands.get(&device.command.model_command.action.device())
                == Some(&device.command.model_command.id)
                && !device.completion_sent
                && measured_target(&plant, device.command.model_command.action)
            {
                device.completion_sent = true;
                events.push(event(
                    now,
                    "device-target-measured",
                    format!(
                        "{{\"id\":{},\"applied_ms\":{},\"measurement\":{},\"error_bound\":0}}",
                        device.command.model_command.id,
                        device.applied_at.as_millis(),
                        match plant.state.value(
                            if device.command.model_command.action.device() == 2 {
                                Variable::Door
                            } else {
                                Variable::Fan
                            }
                        ) {
                            Reading::Number(v) => v.to_string(),
                            _ => unreachable!(),
                        }
                    ),
                ));
                if let Some(feedback) = packet(device, now, config.feedback_delay, true) {
                    feedback_queue.push(feedback);
                }
            }
        }
        feedback_queue.sort_by_key(|f| (f.at, f.physical_id, f.result.sequence));
        let due: Vec<_> = feedback_queue.extract_if(.., |f| f.at <= now).collect();
        for feedback in due {
            let details = format!(
                "{{\"id\":{},\"runtime_command_id\":{},\"sequence\":{},\"observed_ms\":{},\"acknowledged\":true,\"completed\":{}}}",
                feedback.physical_id,
                quote(&feedback.result.command_id),
                feedback.result.sequence,
                feedback.result.observed_at.as_millis(),
                feedback.result.completed
            );
            if name == "lost-feedback" {
                events.push(event(now, "feedback-dropped", details));
                continue;
            }
            if feedback.result.command_id.starts_with("physical-") {
                // Correlated standalone recovery feedback never writes a task
                // outcome. Its physical ID must belong to a released device.
                if !devices.iter().any(|d| {
                    d.command.model_command.id == feedback.physical_id
                        && d.command.runtime_command.is_none()
                }) {
                    return Err("unknown standalone feedback".into());
                }
                if feedback.result.completed {
                    boundaries
                        .iter_mut()
                        .find(|b| b.id == feedback.physical_id)
                        .unwrap()
                        .accepted_completion_at = Some(now);
                }
                events.push(event(now, "persistent-feedback-accepted", details));
                continue;
            }
            match runtime.submit_result(feedback.result.clone()) {
                Ok(()) => {
                    if feedback.result.completed {
                        boundaries
                            .iter_mut()
                            .find(|b| b.id == feedback.physical_id)
                            .unwrap()
                            .accepted_completion_at = Some(now);
                    }
                    events.push(event(now, "feedback-accepted", details));
                }
                Err(IntegratedError::Runtime(
                    ActionError::SupersededCommand | ActionError::StaleResult,
                )) => events.push(event(now, "stale-feedback-rejected", details)),
                Err(e) => return Err(error(e)),
            }
        }
        // The proposal schedule is fixed by public configuration. No true state
        // or scorer result enters this policy boundary.
        if now == config.propose_at {
            supervisor.note_initial_attempt(now);
            proposal_status = format!(
                "{:?}",
                runtime.runtime().assurance().evaluation().assurance_by_node["pressure.direct"]
                    .status()
            );
            match runtime.request_lease("fan-1", ActionPhase::Start) {
                Ok(token) => {
                    runtime
                        .dispatch("fan-1", ActionPhase::Start, &token)
                        .map_err(error)?;
                    start_allowed = true;
                    events.push(event(
                        now,
                        "proposal-allowed",
                        format!("{{\"direct_pressure_status\":{}}}", quote(&proposal_status)),
                    ));
                }
                Err(IntegratedError::Blocked(check)) => events.push(event(
                    now,
                    "proposal-withheld",
                    benchmark::check_json(&check),
                )),
                Err(IntegratedError::Runtime(ActionError::ContractUnavailable(status))) => events
                    .push(event(
                        now,
                        "proposal-withheld",
                        format!("{{\"contract_status\":{}}}", quote(&format!("{status:?}"))),
                    )),
                Err(e) => return Err(error(e)),
            }
        }
        if options.autonomous && now > config.propose_at {
            match supervisor.decide(&runtime) {
                PlanningIntent::OpenThenRetry => {
                    runtime
                        .register_plan(Plan {
                            plan_id: "bounded-replan-1".into(),
                            version: 1,
                            actions: BTreeMap::from([
                                ("open-1".into(), "open".into()),
                                ("fan-2".into(), "fan".into()),
                            ]),
                            precedence: BTreeSet::from([("open-1".into(), "fan-2".into())]),
                            created_at: now,
                        })
                        .map_err(error)?;
                    events.push(event(now,"planner-plan-installed","{\"actions\":[\"DoorOpen\",\"FanStart\"],\"max_replans\":1,\"max_attempts\":4}".into()));
                }
                PlanningIntent::Try(id) => {
                    let attempt = runtime
                        .request_lease(&id, ActionPhase::Start)
                        .and_then(|token| runtime.dispatch(&id, ActionPhase::Start, &token));
                    events.push(event(
                        now,
                        "planner-proposal",
                        format!(
                            "{{\"instance\":{},\"attempt\":{},\"accepted\":{},\"reason\":{}}}",
                            quote(&id),
                            supervisor.attempts(),
                            attempt.is_ok(),
                            quote(&format!("{attempt:?}"))
                        ),
                    ));
                }
                PlanningIntent::Exhausted => {
                    events.push(event(now, "planner-budget-exhausted", "{}".into()))
                }
                PlanningIntent::Wait => {}
            }
        }
        loop {
            match runtime.take_command() {
                Ok(Some(command)) => {
                    let c = &command.model_command;
                    events.push(event(now, "physical-command-released", format!("{{\"id\":{},\"action\":{},\"runtime_command_id\":{},\"max_delay_ms\":{},\"may_drop\":{},\"scheduled_receipt_ms\":{}}}", c.id, quote(&format!("{:?}", c.action)), command.runtime_command.as_ref().map_or("null".into(), |r| quote(&r.command_id)), c.max_delay.as_millis(), c.may_drop, (now + config.command_delay).as_millis())));
                    boundaries.push(ExecutionBoundary {
                        id: c.id,
                        action: c.action,
                        released_at: now,
                        applied_at: None,
                        accepted_completion_at: None,
                    });
                    command_queue.push((now + config.command_delay, command));
                }
                Ok(None) => break,
                Err(IntegratedError::Blocked(check)) => {
                    events.push(event(
                        now,
                        "command-release-blocked",
                        benchmark::check_json(&check),
                    ));
                    break;
                }
                Err(e) => return Err(error(e)),
            }
        }
        for instance in runtime.runtime().instances().values() {
            if instance.state == ActionState::Completed
                && completion_recorded.insert(instance.instance_id.clone())
            {
                let runtime_command =
                    &runtime.runtime().commands()[instance.active_command_id.as_ref().unwrap()];
                let id = devices
                    .iter()
                    .find(|d| {
                        d.command
                            .runtime_command
                            .as_ref()
                            .is_some_and(|c| c.command_id == runtime_command.command_id)
                    })
                    .unwrap()
                    .command
                    .model_command
                    .id;
                completions.push(CompletionClaim {
                    id,
                    claimed_at: now,
                    observed_at: runtime_command.last_observed_at.unwrap(),
                });
            }
        }
        let primary_id = if runtime.runtime().instances().contains_key("fan-2") {
            "fan-2"
        } else {
            "fan-1"
        };
        let instance = &runtime.runtime().instances()[primary_id];
        for c in &runtime.certificates()[certificate_cursor..] {
            events.push(event(now, "certificate-published", certificate_json(c)));
        }
        certificate_cursor = runtime.certificates().len();
        for a in &runtime.runtime().audit()[runtime_cursor..] {
            events.push(event(
                a.at,
                "runtime-audit",
                format!(
                    "{{\"epoch\":{},\"event\":{}}}",
                    a.epoch,
                    quote(&format!("{:?}", a.event))
                ),
            ));
        }
        runtime_cursor = runtime.runtime().audit().len();
        for a in &runtime.audit()[model_cursor..] {
            events.push(event(
                a.recorded_at,
                "model-check",
                format!(
                    "{{\"phase\":{},\"check\":{}}}",
                    quote(a.phase),
                    benchmark::check_json(&a.check)
                ),
            ));
        }
        model_cursor = runtime.audit().len();
        let inside = plant.state.contained_by(runtime.estimator().state());
        contained += usize::from(inside);
        let row = format!(
            "{{\"actual\":{},\"estimate\":{},\"contained\":{},\"inconsistent\":{},\"runtime_state\":{}}}",
            benchmark::truth_json(&plant.state),
            benchmark::box_json(runtime.estimator().state()),
            inside,
            runtime.estimator().inconsistent(),
            quote(&format!("{:?}", instance.state))
        );
        let instances = runtime
            .runtime()
            .instances()
            .values()
            .map(|i| {
                format!(
                    "{{\"id\":{},\"action\":{},\"state\":{}}}",
                    quote(&i.instance_id),
                    quote(&i.action_id),
                    quote(&format!("{:?}", i.state))
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let row = format!("{{\"instances\":[{instances}],{}", &row[1..]);
        events.push(event(now, "boundary-snapshot", row.clone()));
        rows.push(row);
        truth.push(plant.state.clone());
        if now == config.end {
            break;
        }
        plant.step(benchmark::noise(seed, now));
        runtime.advance_to(plant.state.at).map_err(error)?;
    }
    // This is the first scorer call, after every runtime/plant decision is fixed.
    let score =
        execution_score::score_execution(&truth, &boundaries, &completions, Action::FanStart);
    let final_state = runtime
        .runtime()
        .instances()
        .get("fan-2")
        .unwrap_or(&runtime.runtime().instances()["fan-1"])
        .state;
    let configuration_json = format!(
        "{{\"prior\":{},\"initial_actual\":{},\"model_dynamics\":{},\"true_dynamics\":{},\"source_bounds\":[0,0],\"true_source\":0,\"disturbance_scale\":1,\"faults_may_start\":true,\"occupancy_stable_until_ms\":20000,\"propose_at_ms\":{},\"end_ms\":{},\"command_delay_ms\":{},\"sensor_delay_ms\":{},\"feedback_delay_ms\":{},\"pressure_ttl_ms\":500,\"measurement_error_realization\":0,\"device_feedback_error\":0,\"goal\":\"FanStart target >= 0.90 for ventilation test\",\"proposal_policy\":\"one fixed FanStart proposal; configured FanStop recovery\",\"contract_revision\":\"closed-loop-contracts-v1\",\"score_revision\":\"actual-execution-score-v1\"}}",
        benchmark::box_json(&config.prior),
        benchmark::truth_json(&config.actual),
        benchmark::dynamics_json(&config.assumptions.dynamics),
        benchmark::dynamics_json(&config.true_dynamics),
        config.propose_at.as_millis(),
        config.end.as_millis(),
        config.command_delay.as_millis(),
        config.sensor_delay.as_millis(),
        config.feedback_delay.as_millis()
    );
    let configuration_json = configuration_json
        .replace(
            "\"faults_may_start\":true",
            &format!(
                "\"faults_may_start\":{}",
                config.assumptions.faults_may_start
            ),
        )
        .replace(
            "\"pressure_ttl_ms\":500",
            &format!(
                "\"pressure_ttl_ms\":{}",
                if options.benchmark { 750 } else { 500 }
            ),
        )
        .replace(
            "one fixed FanStart proposal; configured FanStop recovery",
            if options.autonomous {
                "bounded event-triggered supervisor; max four proposals and one new plan"
            } else {
                "one fixed FanStart proposal"
            },
        );
    let json = format!(
        "{{\"schema\":{},\"model_version\":{},\"scenario\":{},\"seed\":{},\"source_id\":{},\"config_id\":{},\"configuration\":{},\"start_allowed\":{},\"direct_pressure_status_at_proposal\":{},\"final_runtime_state\":{},\"coverage\":{{\"contained_ticks\":{},\"total_ticks\":{}}},\"score\":{},\"delivered_observations\":[{}],\"events\":[{}],\"trace\":[{}]}}\n",
        quote(CLOSED_LOOP_SCHEMA),
        quote(MODEL_VERSION),
        quote(name),
        seed,
        quote(&source_id()),
        quote(&config_id),
        configuration_json,
        start_allowed,
        quote(&proposal_status),
        quote(&format!("{final_state:?}")),
        contained,
        rows.len(),
        score_json(&score),
        observations.join(","),
        events.join(","),
        rows.join(",")
    );
    let json = format!(
        "{{\"policy_schema\":{},\"policy\":{},\"policy_name\":{},\"autonomous\":{},\"broader_recovery\":{},\"benchmark_ttl_ms\":{},{}",
        quote(POLICY_SCHEMA),
        quote(options.policy.id()),
        quote(options.policy.name()),
        options.autonomous,
        options.broader_recovery,
        if options.benchmark { 750 } else { 500 },
        &json[1..]
    );
    let command_table = score
        .commands
        .iter()
        .map(|c| {
            format!(
                "| {} | {:?} | {} | {} | {} | {:?} | {} |",
                c.id,
                c.action,
                c.released_at.as_millis(),
                maybe_time(c.applied_at),
                maybe_time(c.physical_target_at),
                c.target_by_deadline,
                maybe_time(c.feedback_confirmed_at)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let explanation = match name {
        "hazard-recovery" => {
            "The fixture declares a ventilation obligation and a bounded healthy door. Authorized DoorOpen recovery can preserve ventilation when FanStop is unsupported. Recovery and original task completion remain separate."
        }
        "replanning" => {
            "This controlled fixture declares bounded healthy door behavior. With the bounded supervisor enabled, confirmed cancellation permits one new DoorOpen then FanStart plan while preserving the original task history."
        }
        "normal" => {
            "Fresh sensor messages support the fan test. Acknowledgement confirms device receipt; an exact simulated feedback sensor separately reports the measured fan target."
        }
        "short-gap" => {
            "Pressure messages stop from 500 ms until 4000 ms. At the 1000 ms proposal the direct predicate has expired; a propagated pressure claim can still support release. The actual command and measured response complete the loop."
        }
        "recovery" => {
            "The declared initial door fraction is 0.9 (nearly closed). The nominal equations are unchanged. This fixture tests continued fan supervision and checked stop recovery before predicted pressure risk, rather than assuming an allowed start guarantees task completion."
        }
        "uncertain" => {
            "The actual initial pressure is 0.79, while trusted information admits 0.76 to 0.82. Withholding can be justified by that uncertainty even though the hidden initial state is within the limit. No information-correctness percentage is inferred."
        }
        "lost-feedback" => {
            "Correlated device acknowledgements and completion reports are dropped; regular sensor messages continue. A physical target can be achieved while the runtime correctly leaves the task unconfirmed."
        }
        "delayed-command" => {
            "Command receipt is delayed 600 ms. This reduces recovery headroom and can trigger a checked early stop before the fan-start target. Receipt, physical response and confirmation retain their actual separate times."
        }
        _ => {
            "Sensor readings arrive 300 ms after generation. The estimator applies them at their source times and propagates forward, rather than treating late measurements as current state."
        }
    };
    let policy_label = format!(
        "{}/{} {}",
        POLICY_SCHEMA,
        options.policy.id(),
        options.policy.name()
    );
    let replay_flags = format!(
        "{}{}{}",
        if options.benchmark {
            format!(" --policy {}", options.policy.id())
        } else {
            String::new()
        },
        if options.autonomous {
            " --autonomous"
        } else {
            ""
        },
        if options.broader_recovery {
            " --broader-recovery"
        } else {
            ""
        }
    );
    let markdown = format!(
        "# End-to-end simulation: {name}\n\n{explanation}\n\nSchema `{CLOSED_LOOP_SCHEMA}` · Policy `{policy_label}` · Seed {seed} · Configuration `{config_id}`.\n\nThe runtime **{}** the fixed fan proposal at {} ms. Direct pressure evidence was **{}**. Final task state: **{final_state:?}**.\n\n| Command | Action | Released ms | Applied ms | Physical target ms | Target by deadline | Confirmation received ms |\n| --- | --- | --- | --- | --- | --- | --- |\n{command_table}\n\nIndependent scoring of the recorded execution: pressure-violation ticks **{}**; occupied-closing-motion ticks **{}**; false completion claims **{}**; physical targets without confirmation **{}**; final fan-test objective achieved **{}**. Initial pressure violation: **{}**. State containment: **{contained}/{}** ticks.\n\nPhysics, delayed commands and correlated measured feedback form one loop. Ordinary sensor readings have declared error bounds and zero measurement error in these fixtures; device feedback is an exact simulated sensor. Disturbances are seeded. Fault-onset assumptions and initial hazard are recorded in the configuration. The fan target is predefined. Hardware remains unconnected.\n\nScoring runs after execution and never authorizes an action. Safety fields cover observed windows; JSON identifies incomplete windows and censored deadlines. No 80% correctness, information-sufficiency score, B5/B6 ranking, hardware validation or continuous-time guarantee is claimed. No counterfactual rerun substitutes for the recorded execution.\n\nRead `events.jsonl` for observations, certificates, decisions, release, device receipt, measurements and feedback. `results.json` includes the full configuration and state trace. Reproduce with the same source:\n\n```sh\ncargo run --offline --bin model_demo -- --scenario {name} --seed {seed}{replay_flags} --out NEW_DIRECTORY\n```\n",
        if start_allowed { "allowed" } else { "withheld" },
        config.propose_at.as_millis(),
        proposal_status,
        score.pressure_violation_ticks,
        score.occupied_closing_motion_ticks,
        score.false_completion_claims,
        score.physical_targets_without_confirmation,
        score.objective_achieved,
        score.initial_pressure_violation,
        rows.len()
    );
    Ok(ClosedLoopReport {
        json,
        markdown,
        events_jsonl: events.join("\n") + "\n",
        score,
        final_state,
        contained_ticks: contained,
        total_ticks: rows.len(),
        start_allowed,
    })
}
