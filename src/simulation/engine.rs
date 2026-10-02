use super::*;
use crate::actions::*;
use crate::evidence::{AssuranceStatus as Status, EvidenceAtom};
use crate::runtime::{AuditEvent, EvaluationMode};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
type Result<T> = std::result::Result<T, String>;
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub sequence: u64,
    pub at: Duration,
    pub kind: String,
    pub detail: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reaction {
    pub instance_id: String,
    pub fault_at: Duration,
    pub detected_at: Option<Duration>,
    pub propagated_at: Option<Duration>,
    pub scheduled_at: Option<Duration>,
    pub actuated_at: Option<Duration>,
    pub succeeded: Option<bool>,
    pub margin: Duration,
    pub missed: bool,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metrics {
    pub releases: u64,
    pub unsafe_releases: u64,
    pub false_safe_releases: u64,
    pub useful_opportunities: BTreeSet<String>,
    pub retained_opportunities: BTreeSet<String>,
    pub replans: u64,
    pub fallbacks: u64,
    pub witness_switches: u64,
    pub lease_renewals: u64,
    pub revocations: u64,
    pub unnecessary_revocations: u64,
    pub reactions: Vec<Reaction>,
}
#[derive(Debug, Clone)]
pub struct WorkRecord {
    pub at: Duration,
    pub evidence: String,
    pub nodes: usize,
    pub rules: usize,
    pub affected_rules: usize,
    pub nodes_recomputed: usize,
    pub rules_recomputed: usize,
    pub elapsed_ns: u128,
}
#[derive(Debug, Clone)]
enum Event {
    Sample,
    Observation(EvidenceAtom, Link),
    Authority(String, ActionPhase, LeaseToken),
    AuthorityExpired(String, String),
    Result(ActuatorResult),
    Deliver(String, Status, Duration),
    Wake,
    Occupant,
    Drop(String),
    Partition,
    HumanUnavailable,
    Loss,
    Detect(usize),
    Propagate(usize),
    Deadline(usize),
    SendCommand(ActuatorCommand),
    Execute(ActuatorCommand),
    Ack(ActuatorCommand),
    Finish(ActuatorCommand),
    Commit(String),
    Replan,
}
impl Event {
    fn priority(&self) -> u8 {
        match self {
            Self::Occupant
            | Self::Drop(_)
            | Self::Partition
            | Self::HumanUnavailable
            | Self::Loss => 0,
            Self::Sample
            | Self::Observation(..)
            | Self::Deliver(..)
            | Self::Wake
            | Self::Detect(_)
            | Self::Propagate(_) => 1,
            Self::SendCommand(_)
            | Self::Execute(_)
            | Self::Ack(_)
            | Self::Finish(_)
            | Self::Deadline(_) => 2,
            Self::Result(_) => 2,
            Self::Authority(..) | Self::AuthorityExpired(..) | Self::Commit(_) | Self::Replan => 3,
        }
    }
}
/// All policies expose the same lifecycle operations; only admission graphs vary.
pub trait AdmissionRuntime {
    fn on_evidence_update(&mut self, atom: EvidenceAtom) -> std::result::Result<(), ActionError>;
    fn request_start(&mut self, id: &str) -> std::result::Result<String, ActionError>;
    fn request_commit(&mut self, id: &str) -> std::result::Result<String, ActionError>;
    fn on_action_outcome(&mut self, result: ActuatorResult)
    -> std::result::Result<(), ActionError>;
}
impl AdmissionRuntime for ActionRuntime {
    fn on_evidence_update(&mut self, atom: EvidenceAtom) -> std::result::Result<(), ActionError> {
        self.update(atom)
    }
    fn request_start(&mut self, id: &str) -> std::result::Result<String, ActionError> {
        let l = self.request_lease(id, ActionPhase::Start)?;
        self.dispatch(id, ActionPhase::Start, &l)
    }
    fn request_commit(&mut self, id: &str) -> std::result::Result<String, ActionError> {
        let l = self.request_lease(id, ActionPhase::Commit)?;
        self.dispatch(id, ActionPhase::Commit, &l)
    }
    fn on_action_outcome(&mut self, r: ActuatorResult) -> std::result::Result<(), ActionError> {
        self.submit_result(r)
    }
}
pub struct Simulation {
    pub config: SimulationConfig,
    pub network: Network,
    pub ablation: Ablation,
    source_versions: BTreeMap<String, u64>,
    pending_authority: BTreeMap<String, String>,
    executed_commands: BTreeSet<String>,
    actuator_current: BTreeMap<String, ActuatorCommand>,
    physical_inputs: BTreeMap<String, String>,
    runtime: ActionRuntime,
    truth: BuildingState,
    estimate: StateEstimate,
    queue: BTreeMap<(Duration, u8, u64), Event>,
    next_event: u64,
    pub events: Vec<EventRecord>,
    pub metrics: Metrics,
    pub work: Vec<WorkRecord>,
    pub revalidation_ns: Vec<u128>,
    pub lease_issuance_ns: Vec<u128>,
    pub journal: Vec<String>,
    dropped: BTreeSet<String>,
    ack_dropped: BTreeSet<String>,
    override_status: BTreeMap<String, Status>,
    commit_due: BTreeSet<String>,
    action_offset: usize,
    assurance_offset: usize,
    work_offset: usize,
    issued: BTreeSet<(String, String)>,
    rng: u64,
}
impl Simulation {
    pub fn new(config: SimulationConfig) -> Result<Self> {
        Self::with_mode(config, EvaluationMode::Incremental)
    }
    pub fn with_mode(config: SimulationConfig, mode: EvaluationMode) -> Result<Self> {
        Self::with_experiment(config, mode, NetworkConfig::default(), Ablation::None)
    }
    pub fn with_experiment(
        config: SimulationConfig,
        mode: EvaluationMode,
        network: NetworkConfig,
        ablation: Ablation,
    ) -> Result<Self> {
        if ablation != Ablation::None && config.baseline != Baseline::B5 {
            return Err("feature removals apply only to B5 comparisons".into());
        }
        let network = Network::new(network, config.seed)?;
        if ![
            "S1", "S2", "S3", "S4", "S5", "C11", "C12", "C13", "C14", "C15", "C16",
        ]
        .contains(&config.scenario.as_str())
        {
            return Err("unknown scenario (S1–S5 or C11–C16)".into());
        }
        let (g, d) = experiment_configuration(config.baseline, ablation);
        let mut runtime = ActionRuntime::with_mode(g, d, mode).map_err(|e| e.to_string())?;
        runtime.enable_work_log();
        let mut s = Self {
            rng: config.seed.max(1),
            config,
            network,
            ablation,
            source_versions: BTreeMap::new(),
            pending_authority: BTreeMap::new(),
            executed_commands: BTreeSet::new(),
            actuator_current: BTreeMap::new(),
            physical_inputs: BTreeMap::new(),
            runtime,
            truth: BuildingState::default(),
            estimate: StateEstimate::default(),
            queue: BTreeMap::new(),
            next_event: 0,
            events: Vec::new(),
            metrics: Metrics::default(),
            work: Vec::new(),
            revalidation_ns: Vec::new(),
            lease_issuance_ns: Vec::new(),
            journal: Vec::new(),
            dropped: BTreeSet::new(),
            ack_dropped: BTreeSet::new(),
            override_status: BTreeMap::new(),
            commit_due: BTreeSet::new(),
            action_offset: 0,
            assurance_offset: 0,
            work_offset: 0,
            issued: BTreeSet::new(),
        };
        let plan = ScriptedPlanner.propose(&s.config.scenario, &s.estimate, &[], 1);
        s.runtime.register_plan(plan).map_err(|e| e.to_string())?;
        s.observe("always", Status::Valid, Duration::from_secs(86400))?;
        s.schedule(Duration::ZERO, Event::Sample);
        match s.config.scenario.as_str() {
            "S2" => s.schedule(ms(500), Event::Drop("s1".into())),
            "S3" => {
                s.schedule(ms(1040), Event::Occupant);
            }
            "S4" => s.schedule(ms(500), Event::Partition),
            "S5" => {
                s.schedule(ms(500), Event::Drop("s1".into()));
                s.schedule(ms(700), Event::HumanUnavailable);
                s.schedule(ms(1100), Event::Occupant);
            }
            "C13" | "C14" => s.schedule(ms(500), Event::Loss),
            "C15" => s.schedule(ms(600), Event::Partition),
            "C16" => s.schedule(ms(800), Event::Replan),
            _ => {}
        }
        s.flush();
        Ok(s)
    }
    pub fn runtime(&self) -> &ActionRuntime {
        &self.runtime
    }
    pub fn ground_truth(&self) -> &BuildingState {
        &self.truth
    }
    pub fn estimate(&self) -> &StateEstimate {
        &self.estimate
    }
    pub fn now(&self) -> Duration {
        self.runtime.now()
    }
    fn log(&mut self, kind: &str, detail: impl Into<String>) {
        self.events.push(EventRecord {
            sequence: self.events.len() as u64 + 1,
            at: self.now(),
            kind: kind.into(),
            detail: detail.into(),
        });
    }
    fn schedule(&mut self, at: Duration, event: Event) {
        self.next_event += 1;
        self.queue
            .insert((at, event.priority(), self.next_event), event);
    }
    fn random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }
    fn make_atom(&mut self, id: &str, status: Status, expires: Duration) -> EvidenceAtom {
        let version = self.source_versions.entry(id.into()).or_default();
        *version += 1;
        EvidenceAtom {
            evidence_id: id.into(),
            evidence_type: "simulation".into(),
            source_id: format!("sensor.{id}"),
            predicate: id.into(),
            version: *version,
            observed_at: self.now(),
            expires_at: expires,
            status,
            payload_hash: None,
        }
    }
    fn observe(&mut self, id: &str, status: Status, expires: Duration) -> Result<()> {
        let atom = self.make_atom(id, status, expires);
        self.accept_observation(atom)
    }
    fn accept_observation(&mut self, atom: EvidenceAtom) -> Result<()> {
        let id = atom.evidence_id.clone();
        let previous = self.estimate.status(&id, self.now());
        let (status, expires) = (atom.status, atom.expires_at);
        self.runtime
            .on_evidence_update(atom)
            .map_err(|e| e.to_string())?;
        self.estimate.0.insert(id.clone(), (status, expires));
        let g = self.runtime.assurance().graph();
        // Reachability is diagnostic and kept outside the measured update call.
        let mut affected = BTreeSet::from([id.to_string()]);
        let mut rules = BTreeSet::new();
        loop {
            let n = affected.len();
            for (rid, r) in g.justifications() {
                if r.premises.iter().any(|p| affected.contains(p)) {
                    rules.insert(rid.clone());
                    affected.insert(r.conclusion.clone());
                }
            }
            if affected.len() == n {
                break;
            }
        }

        if self.config.baseline == Baseline::B2 && previous != self.estimate.status(&id, self.now())
        {
            let stop: Vec<_> = self
                .runtime
                .instances()
                .values()
                .filter(|i| matches!(i.state, ActionState::Executing | ActionState::Committed))
                .filter(|i| {
                    let root = self.runtime.definitions()[&i.action_id]
                        .run_contract
                        .as_ref();
                    root.is_some_and(|r| affected.contains(r))
                })
                .map(|i| i.instance_id.clone())
                .collect();
            for id in stop {
                self.runtime
                    .request_stop(&id, status)
                    .map_err(|e| e.to_string())?;
            }
        }
        if status == Status::Valid && expires > self.now() {
            self.schedule(expires, Event::Wake);
        }
        self.flush();
        Ok(())
    }
    fn transmit(&mut self, link: Link, key: &str, event: Event) {
        let arrivals = self.network.send(link, key, self.now());
        let transmission = self.network.transmissions.last().unwrap();
        self.log("NetworkSend", format!("{transmission:?}"));
        for at in arrivals {
            self.schedule(at, event.clone());
        }
    }
    fn receive_allowed(&mut self, link: Link, key: &str) -> bool {
        if self.network.config.partitioned(link, self.now()) {
            self.log(
                "NetworkDrop",
                format!("link={link:?} key={key} partition at receipt"),
            );
            false
        } else {
            true
        }
    }
    fn authority_key(id: &str, phase: ActionPhase) -> String {
        format!("{id}:{phase:?}")
    }
    fn active_actions(&self, except: &str) -> Vec<String> {
        self.runtime
            .instances()
            .values()
            .filter(|i| {
                i.instance_id != except
                    && matches!(
                        i.state,
                        ActionState::Leased
                            | ActionState::Executing
                            | ActionState::Committed
                            | ActionState::Aborting
                            | ActionState::Recovering
                    )
            })
            .map(|i| i.action_id.clone())
            .collect()
    }
    fn update_physics(&mut self) -> Result<()> {
        for a in ACTIONS {
            // Only start contracts use B4 physics; refreshing this estimate cannot
            // revoke a consumed lease or monitor its running/commit phases.
            let self_id = self
                .runtime
                .instances()
                .values()
                .find(|i| i.action_id == a && !i.state.is_terminal())
                .map(|i| i.instance_id.as_str())
                .unwrap_or("");
            let value = BuildingOracle.evaluate(
                a,
                &self.estimate,
                &self.active_actions(self_id),
                self.now(),
            );
            let id = format!("{a}.safe");
            let inputs = format!(
                "{:?}:{:?}",
                self.estimate
                    .0
                    .iter()
                    .filter(|(id, _)| physical_inputs(a).contains(&id.as_str()))
                    .collect::<Vec<_>>(),
                self.active_actions(self_id)
            );
            let unchanged = self.physical_inputs.get(a) == Some(&inputs)
                && self.estimate.0.get(&id).is_some_and(|(status, until)| {
                    *status == value.status() && (*status != Status::Valid || *until > self.now())
                });
            if unchanged {
                continue;
            }
            self.physical_inputs.insert(a.into(), inputs);
            let expiry = value.horizon().unwrap_or(self.now() + ms(1000));
            if self.estimate.0.get(&id) != Some(&(value.status(), expiry)) {
                self.observe(&id, value.status(), expiry)?;
            }
        }
        Ok(())
    }
    fn truth_support(&self, a: &str, phase: ActionPhase) -> (bool, bool) {
        let estimate = StateEstimate(
            SENSORS
                .iter()
                .map(|id| {
                    (
                        id.to_string(),
                        (self.truth.sample(id), self.now() + ms(1000)),
                    )
                })
                .collect(),
        );
        // Suppression start only prepares the actuator. Occupancy constrains
        // the irreversible commit, not this reversible preparation.
        let physical = (a == "discharge" && phase == ActionPhase::Start)
            || BuildingOracle
                .evaluate(a, &estimate, &[], self.now())
                .status()
                == Status::Valid;
        let hazard = self.truth.hazard[1];
        let logical = match (a, phase) {
            ("alarm" | "light", _) => hazard,
            ("fan", _) => hazard && self.truth.fan_healthy,
            ("discharge", ActionPhase::Start) => self.truth.suppression_armed,
            ("discharge", ActionPhase::Commit) => {
                hazard
                    && !self.truth.occupied[1]
                    && self.truth.alarm_active
                    && self.truth.path_healthy
                    && self.truth.operator_approved
            }
            _ => true,
        } && self.truth.agent_available;
        (physical, physical && logical)
    }
    fn release(&mut self, id: &str, phase: ActionPhase) -> bool {
        let key = Self::authority_key(id, phase);
        if self.pending_authority.contains_key(&key) {
            return false;
        }
        let action = self.runtime.instances()[id].action_id.clone();
        if phase == ActionPhase::Start && self.truth_support(&action, phase).1 {
            self.metrics.useful_opportunities.insert(id.into());
        }
        let t = Instant::now();
        let result = self.runtime.request_lease(id, phase);
        self.lease_issuance_ns.push(t.elapsed().as_nanos());
        if let Ok(token) = result {
            let expiry = self.runtime.leases()[&token.lease_id].expires_at;
            self.pending_authority
                .insert(key.clone(), token.lease_id.clone());
            self.schedule(
                expiry,
                Event::AuthorityExpired(key.clone(), token.lease_id.clone()),
            );
            self.transmit(
                Link::Authority,
                &format!("{key}:{}", token.lease_id),
                Event::Authority(id.into(), phase, token),
            );
            true
        } else {
            false
        }
    }
    fn dispatch_authority(&mut self, id: &str, phase: ActionPhase, token: LeaseToken) {
        let key = Self::authority_key(id, phase);
        if self.pending_authority.get(&key) == Some(&token.lease_id) {
            self.pending_authority.remove(&key);
        }
        if !self.receive_allowed(Link::Authority, &key) {
            return;
        }
        let action = self.runtime.instances()[id].action_id.clone();
        let (physical, supported) = self.truth_support(&action, phase);
        let physical = physical
            && self
                .active_actions(id)
                .iter()
                .all(|b| joint_safe(&action, b));
        let start = Instant::now();
        let result = self.runtime.dispatch(id, phase, &token);
        self.revalidation_ns.push(start.elapsed().as_nanos());
        if result.is_ok() {
            self.metrics.releases += 1;
            self.metrics.unsafe_releases += u64::from(!physical);
            self.metrics.false_safe_releases += u64::from(!physical || !supported);
            if supported && phase == ActionPhase::Start {
                self.metrics.retained_opportunities.insert(id.into());
            }
            if phase == ActionPhase::Commit {
                self.commit_due.remove(id);
            }
            self.log("Release",format!("instance={id} phase={phase:?} ground_truth_physical={physical} ground_truth_supported={supported}"));
        } else {
            self.log(
                "AuthorityRejected",
                format!("instance={id} lease={} reason={result:?}", token.lease_id),
            );
        }
    }
    fn admit(&mut self) -> Result<()> {
        self.update_physics()?;
        let ids: Vec<_> = self
            .runtime
            .instances()
            .values()
            .filter(|i| matches!(i.state, ActionState::Pending | ActionState::Ready))
            .map(|i| i.instance_id.clone())
            .collect();
        let candidates: Vec<_> = ids
            .iter()
            .filter_map(|id| {
                let i = &self.runtime.instances()[id];
                (i.state == ActionState::Ready).then(|| i.action_id.clone())
            })
            .collect();
        let selected = supported_subset(&candidates, &self.active_actions(""));
        for id in ids {
            let i = &self.runtime.instances()[&id];
            let predecessors = self.runtime.plans()[&i.plan_id]
                .precedence
                .iter()
                .filter(|(_, to)| to == &id)
                .all(|(from, _)| self.runtime.instances()[from].state == ActionState::Completed);
            if !predecessors {
                continue;
            }
            if self.config.baseline == Baseline::B5
                && self.ablation != Ablation::NoPhysical
                && i.state == ActionState::Ready
                && !selected.contains(&i.action_id)
            {
                let action = i.action_id.clone();
                if self.truth_support(&action, ActionPhase::Start).1 {
                    self.metrics.useful_opportunities.insert(id.clone());
                }
                self.log("ConcurrencyDeferred", id);
                continue;
            }
            self.release(&id, ActionPhase::Start);
            // Reservations from a preceding release affect later candidates at this timestamp.
            self.update_physics()?;
        }
        let due: Vec<_> = self.commit_due.iter().cloned().collect();
        for id in due {
            if self.runtime.instances()[&id].state == ActionState::Executing {
                self.release(&id, ActionPhase::Commit);
            }
        }
        self.flush();
        self.drain();
        Ok(())
    }
    fn drain(&mut self) {
        while let Some(command) = self.runtime.take_command() {
            let recovery = !matches!(command.kind, CommandKind::Start | CommandKind::Commit);
            if recovery {
                self.metrics.fallbacks += 1;
                self.metrics.revocations += 1;
                let action = &self.runtime.instances()[&command.instance_id].action_id;
                let supported = match action.as_str() {
                    "fan" => self.truth.fan_healthy && self.truth.pressure >= 50,
                    "door" => self.truth.agent_available && self.truth.egress_clear,
                    _ => self.truth_support(action, ActionPhase::Start).1,
                };
                self.metrics.unnecessary_revocations += u64::from(supported);
                if !self
                    .metrics
                    .reactions
                    .iter()
                    .any(|r| r.instance_id == command.instance_id && r.actuated_at.is_none())
                {
                    let margin = if command.kind == CommandKind::CertifiedComplete {
                        ms(3000)
                    } else {
                        ms(500)
                    };
                    let n = self.metrics.reactions.len();
                    self.metrics.reactions.push(Reaction {
                        instance_id: command.instance_id.clone(),
                        fault_at: command.issued_at,
                        detected_at: Some(command.issued_at),
                        propagated_at: Some(command.issued_at),
                        scheduled_at: None,
                        actuated_at: None,
                        succeeded: None,
                        margin,
                        missed: false,
                    });
                    self.schedule(command.issued_at + margin, Event::Deadline(n));
                }
            }
            let delay = if recovery { ms(10) } else { Duration::ZERO };
            self.schedule(self.now() + delay, Event::SendCommand(command));
        }
        self.flush();
    }
    fn actuator_current(&self, c: &ActuatorCommand) -> bool {
        self.actuator_current
            .get(&c.instance_id)
            .is_some_and(|active| active.command_id == c.command_id)
    }
    fn respond(&mut self, c: &ActuatorCommand, completed: bool, success: bool) -> Result<()> {
        let a = self.runtime.instances()[&c.instance_id].action_id.clone();
        let ack = if self.ack_dropped.contains(&a) {
            None
        } else {
            Some(success)
        };
        let fields = if matches!(
            c.kind,
            CommandKind::Start | CommandKind::Commit | CommandKind::CertifiedComplete
        ) {
            BTreeMap::from([(
                "done".into(),
                if completed && success {
                    Status::Valid
                } else {
                    Status::Unknown
                },
            )])
        } else {
            BTreeMap::new()
        };
        let result = ActuatorResult {
            command_id: c.command_id.clone(),
            sequence: if completed { 2 } else { 1 },
            acknowledged: ack,
            observed_state: fields,
            completed,
            observed_at: self.now(),
        };
        self.transmit(
            Link::Result,
            &format!("{}:{}", result.command_id, result.sequence),
            Event::Result(result),
        );
        self.flush();
        Ok(())
    }
    fn apply(&mut self, event: Event) -> Result<()> {
        self.log("Event", format!("{event:?}"));
        let old_truth = self.truth.clone();
        match event {
            Event::Sample => {
                for id in SENSORS {
                    if self.dropped.contains(id) {
                        continue;
                    }
                    if self.config.scenario == "S3"
                        && id == "occupancy_clear"
                        && self.now() > Duration::ZERO
                    {
                        continue;
                    }
                    if self.config.scenario == "S5"
                        && self.now() > ms(500)
                        && self.random().is_multiple_of(5)
                    {
                        self.log("PacketDropped", id);
                        continue;
                    }
                    let status = self
                        .override_status
                        .get(id)
                        .copied()
                        .unwrap_or_else(|| self.truth.sample(id));
                    let ttl = if self.config.scenario == "S3" && id == "occupancy_clear" {
                        ms(1050)
                    } else {
                        ms(2000)
                    };
                    self.schedule(
                        self.now(),
                        Event::Deliver(id.into(), status, self.now() + ttl),
                    );
                }
                self.schedule(self.now() + ms(500), Event::Sample);
            }
            Event::Deliver(id, status, expiry) => {
                let atom = self.make_atom(&id, status, expiry);
                let link = if id == "agent_available" {
                    Link::Cloud
                } else {
                    Link::Sensor
                };
                self.transmit(
                    link,
                    &format!("sensor:{id}:{}", self.now().as_nanos()),
                    Event::Observation(atom, link),
                );
            }
            Event::Observation(atom, link) => {
                if self.receive_allowed(link, &atom.evidence_id) {
                    // Keep source metadata. A stale/duplicate observation cannot replace a newer one.
                    let id = atom.evidence_id.clone();
                    let version = atom.version;
                    if self
                        .runtime
                        .assurance()
                        .evidence()
                        .get(&id)
                        .is_some_and(|a| a.version >= version)
                    {
                        self.log(
                            "ObservationRejected",
                            format!("id={id} version={version} stale or duplicate"),
                        );
                    } else {
                        self.accept_observation(atom)?;
                    }
                }
            }
            Event::Authority(id, phase, token) => self.dispatch_authority(&id, phase, token),
            Event::AuthorityExpired(key, lease) => {
                if self.pending_authority.get(&key) == Some(&lease) {
                    self.pending_authority.remove(&key);
                }
            }
            Event::Result(result) => {
                if self.receive_allowed(Link::Result, &result.command_id) {
                    let id = result.command_id.clone();
                    let accepted = self.runtime.on_action_outcome(result);
                    match accepted {
                        Ok(()) => {}
                        Err(ActionError::StaleResult | ActionError::SupersededCommand) => self.log(
                            "ResultRejected",
                            format!("command={id} stale, duplicate or superseded"),
                        ),
                        Err(e) => return Err(e.to_string()),
                    }
                }
            }
            Event::SendCommand(command) => {
                self.transmit(
                    Link::Actuator,
                    &command.command_id.clone(),
                    Event::Execute(command),
                );
            }
            Event::Wake => {}
            Event::Occupant => self.truth.occupied[1] = true,
            Event::Drop(id) => {
                self.dropped.insert(id.clone());
                self.observe(&id, Status::Unknown, self.now() + ms(2000))?;
            }
            Event::Partition => {
                self.truth.agent_available = false;
                self.override_status
                    .insert("agent_available".into(), Status::Unknown);
                self.observe("agent_available", Status::Unknown, self.now() + ms(2000))?;
            }
            Event::HumanUnavailable => {
                self.truth.operator_approved = false;
                self.override_status
                    .insert("operator_approved".into(), Status::Unknown);
                self.observe("operator_approved", Status::Unknown, self.now() + ms(2000))?;
            }
            Event::Loss => {
                self.dropped.insert("pressure_safe".into());
                let i = self.metrics.reactions.len();
                let instance_id = self
                    .runtime
                    .instances()
                    .values()
                    .find(|i| i.action_id == "fan")
                    .unwrap()
                    .instance_id
                    .clone();
                self.metrics.reactions.push(Reaction {
                    instance_id,
                    fault_at: self.now(),
                    detected_at: None,
                    propagated_at: None,
                    scheduled_at: None,
                    actuated_at: None,
                    succeeded: None,
                    margin: ms(500),
                    missed: false,
                });
                self.schedule(self.now() + ms(50), Event::Detect(i));
                self.schedule(self.now() + ms(500), Event::Deadline(i));
            }
            Event::Detect(i) => {
                self.metrics.reactions[i].detected_at = Some(self.now());
                self.schedule(self.now() + ms(20), Event::Propagate(i));
            }
            Event::Propagate(i) => {
                self.metrics.reactions[i].propagated_at = Some(self.now());
                self.observe("pressure_safe", Status::Unknown, self.now() + ms(2000))?;
            }
            Event::Deadline(i) => {
                if self.metrics.reactions[i].succeeded != Some(true)
                    || self.metrics.reactions[i].actuated_at.is_none_or(|t| {
                        t - self.metrics.reactions[i].fault_at >= self.metrics.reactions[i].margin
                    })
                {
                    self.metrics.reactions[i].missed = true;
                    self.log("ReactionDeadlineMiss", format!("reaction={i}"));
                }
            }
            Event::Commit(id) => {
                self.commit_due.insert(id);
            }
            Event::Replan => self.replan()?,
            Event::Execute(c) => {
                if !self.receive_allowed(Link::Actuator, &c.command_id) {
                    return Ok(());
                }
                if !self.executed_commands.insert(c.command_id.clone()) {
                    self.log("DuplicateCommandIgnored", &c.command_id);
                    return Ok(());
                }
                // The device only knows commands that reached it. A runtime
                // cancellation does not magically cancel an in-flight command.
                let serial = |command: &ActuatorCommand| {
                    command
                        .command_id
                        .strip_prefix("command-")
                        .unwrap()
                        .parse::<u64>()
                        .unwrap()
                };
                if self
                    .actuator_current
                    .get(&c.instance_id)
                    .is_some_and(|old| serial(old) > serial(&c))
                {
                    self.log("ObsoleteCommandIgnored", &c.command_id);
                    return Ok(());
                }
                self.actuator_current
                    .insert(c.instance_id.clone(), c.clone());
                let i = &self.runtime.instances()[&c.instance_id];
                let a = i.action_id.clone();
                let start = i.started_at.unwrap();
                let recovery = !matches!(c.kind, CommandKind::Start | CommandKind::Commit);
                if recovery {
                    let now = self.now();
                    for r in &mut self.metrics.reactions {
                        if r.instance_id == c.instance_id
                            && r.propagated_at.is_some()
                            && r.scheduled_at.is_none()
                        {
                            r.scheduled_at = Some(now);
                        }
                    }
                }
                match c.kind {
                    CommandKind::Start => {
                        match a.as_str() {
                            "fan" => self.truth.fan_starting = true,
                            "door" => self.truth.door_closing = true,
                            "damper" => self.truth.damper_closing = true,
                            _ => {}
                        }
                        self.schedule(self.now() + ms(10), Event::Ack(c.clone()));
                        if self.runtime.definitions()[&a].commit_contract.is_some() {
                            self.schedule(
                                self.now() + if a == "door" { ms(500) } else { ms(100) },
                                Event::Commit(c.instance_id.clone()),
                            );
                        } else {
                            self.schedule(self.now() + duration(&a), Event::Finish(c));
                        }
                    }
                    CommandKind::Commit => {
                        self.schedule(self.now() + ms(10), Event::Ack(c.clone()));
                        self.schedule(
                            (start + duration(&a)).max(self.now() + ms(10)),
                            Event::Finish(c),
                        );
                    }
                    _ => {
                        let delay = if self.config.scenario == "C14" {
                            ms(650)
                        } else {
                            ms(100)
                        };
                        let at = if c.kind == CommandKind::CertifiedComplete {
                            (start + duration(&a)).max(self.now() + delay)
                        } else {
                            self.now() + delay
                        };
                        self.schedule(at, Event::Finish(c));
                    }
                }
            }
            Event::Ack(c) => self.respond(&c, false, true)?,
            Event::Finish(c) => {
                if !self.actuator_current(&c) {
                    self.log("SupersededPhysicalEvent", &c.command_id);
                    return Ok(());
                }
                let a = self.runtime.instances()[&c.instance_id].action_id.clone();
                let success = !(c.kind == CommandKind::CertifiedComplete
                    && a == "door"
                    && !self.truth.egress_clear);
                match c.kind {
                    CommandKind::Start | CommandKind::Commit | CommandKind::CertifiedComplete => {
                        if success {
                            self.truth.finish(&a);
                        }
                    }
                    _ => self.truth.abort(&a),
                }
                if !matches!(c.kind, CommandKind::Start | CommandKind::Commit) {
                    let now = self.now();
                    for r in &mut self.metrics.reactions {
                        if r.instance_id == c.instance_id
                            && r.scheduled_at.is_some()
                            && r.actuated_at.is_none()
                        {
                            r.actuated_at = Some(now);
                            r.succeeded = Some(success);
                        }
                    }
                }
                self.respond(&c, true, success)?;
                // Local sensors observe resulting plant state, separately from correlated outcome bindings.
                for id in [
                    "alarm_active",
                    "suppression_armed",
                    "fan_off",
                    "damper_open",
                ] {
                    if !self.dropped.contains(id) {
                        self.schedule(
                            self.now(),
                            Event::Deliver(id.into(), self.truth.sample(id), self.now() + ms(2000)),
                        );
                    }
                }
            }
        }
        if old_truth != self.truth {
            self.log(
                "PlantChanged",
                format!("old={old_truth:?} new={:?}", self.truth),
            );
        }
        for violation in self.truth.violations() {
            self.log("InvariantViolation", violation);
        }
        self.flush();
        self.drain();
        Ok(())
    }
    fn flush(&mut self) {
        for w in &self.runtime.assurance().work_log()[self.work_offset..] {
            self.work.push(WorkRecord {
                at: w.at,
                evidence: w.changed.join("|"),
                nodes: w.nodes,
                rules: w.rules,
                affected_rules: w.affected_rules,
                nodes_recomputed: w.stats.nodes_evaluated,
                rules_recomputed: w.stats.justifications_evaluated,
                elapsed_ns: w.elapsed_ns,
            });
        }
        self.work_offset = self.runtime.assurance().work_log().len();
        let entries = self.runtime.audit()[self.action_offset..].to_vec();
        self.action_offset = self.runtime.audit().len();
        for entry in entries {
            match &entry.event {
                ActionEvent::LeaseIssued { lease_id } => {
                    let l = &self.runtime.leases()[lease_id];
                    if !self
                        .issued
                        .insert((l.action_instance_id.clone(), format!("{:?}", l.phase)))
                    {
                        self.metrics.lease_renewals += 1;
                    }
                }
                ActionEvent::LeaseRevoked { lease_id, .. } => {
                    self.metrics.revocations += 1;
                    let l = &self.runtime.leases()[lease_id];
                    let a = &self.runtime.instances()[&l.action_instance_id].action_id;
                    if self.truth_support(a, l.phase).1 {
                        self.metrics.unnecessary_revocations += 1;
                    }
                }
                _ => {}
            }
            self.events.push(EventRecord {
                sequence: self.events.len() as u64 + 1,
                at: entry.at,
                kind: "ActionAudit".into(),
                detail: format!("{:?}", entry.event),
            });
        }
        let entries = self.runtime.assurance().audit()[self.assurance_offset..].to_vec();
        self.assurance_offset = self.runtime.assurance().audit().len();
        for entry in entries {
            if let AuditEvent::WitnessChanged {
                previous: Some(p),
                current: Some(c),
                ..
            } = &entry.event
                && (p.evidence_ids != c.evidence_ids || p.justification_id != c.justification_id)
            {
                self.metrics.witness_switches += 1;
            }
            self.events.push(EventRecord {
                sequence: self.events.len() as u64 + 1,
                at: entry.at,
                kind: "AssuranceAudit".into(),
                detail: format!("{:?}", entry.event),
            });
        }
    }
    fn advance_to(&mut self, target: Duration) -> Result<()> {
        if target < self.now() {
            return Err("time cannot go backwards".into());
        }
        loop {
            let queued = self.queue.first_key_value().map(|(k, _)| k.0);
            let next = queued.into_iter().chain(self.runtime.next_deadline()).min();
            let Some(at) = next.filter(|t| *t <= target) else {
                break;
            };
            self.runtime.advance_to(at).map_err(|e| e.to_string())?;
            self.flush();
            self.drain();
            while self.queue.first_key_value().is_some_and(|(k, _)| k.0 == at) {
                let (_, e) = self.queue.pop_first().unwrap();
                self.apply(e)?;
            }
            self.admit()?;
            // Commands enqueued by admission are processed before time advances.
        }
        self.runtime.advance_to(target).map_err(|e| e.to_string())?;
        self.flush();
        self.drain();
        Ok(())
    }
    fn replan(&mut self) -> Result<()> {
        let version = self.runtime.plans()["building"].version + 1;
        let history: Vec<_> = self.runtime.instances().values().cloned().collect();
        let plan =
            ScriptedPlanner.propose(&self.config.scenario, &self.estimate, &history, version);
        self.runtime
            .replace_pending_suffix("building", version, plan.actions, plan.precedence)
            .map_err(|e| e.to_string())?;
        self.metrics.replans += 1;
        self.log(
            "Replan",
            format!(
                "version={version} estimate={:?} history={history:?}",
                self.estimate
            ),
        );
        self.flush();
        Ok(())
    }
    pub fn status(&self) -> String {
        format!(
            "time={:?}\nGround truth (experiment observer only): {:?}\n{}",
            self.now(),
            self.truth,
            self.runtime
                .instances()
                .values()
                .map(|i| format!("{}: {:?}", i.instance_id, i.state))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
    /// Process a terminal command. Only successfully applied mutations enter replay.
    pub fn command(&mut self, line: &str) -> Result<String> {
        let words: Vec<_> = line.split_whitespace().collect();
        let mut mutation = false;
        let result=match words.as_slice() {
            ["help"]=>"status | actions | evidence | explain NODE | events [N] | advance 500ms | step | run 10s | sensor ID valid|unknown|invalid [TTL] | fault ID drop|restore | ack ACTION drop|restore | plant FIELD VALUE | replan | save DIRECTORY | quit".into(),
            ["status"]|["actions"]=>self.status(),
            ["evidence"]=>format!("{:#?}",self.runtime.assurance().evidence()),
            ["explain",id]=>{let v=self.runtime.assurance().evaluation().assurance_by_node.get(*id).ok_or("unknown node")?;format!("{id}: {v:?}\nwitness: {:?}",self.runtime.assurance().evaluation().preferred_witness_by_node.get(*id))},
            ["events"]=>self.recent_events(20),["events",n]=>self.recent_events(n.parse().map_err(|_|"invalid event count")?),
            ["advance"|"run",amount]=>{let d=parse_duration(amount)?;let target=self.now().checked_add(d).ok_or("time overflow")?;self.advance_to(target)?;mutation=true;self.status()},
            ["step"]=>{let at=self.queue.first_key_value().map(|(k,_)|k.0).into_iter().chain(self.runtime.next_deadline()).min().ok_or("no events")?;self.advance_to(at)?;mutation=true;self.status()},
            ["sensor",id,status] | ["sensor",id,status,_]=>{
                if !SENSORS.contains(id){return Err("unknown sensor (use evidence)".into());}
                let s=match *status {"valid"=>Status::Valid,"unknown"=>Status::Unknown,"invalid"=>Status::Invalid,_=>return Err("status must be valid, unknown or invalid".into())};
                let ttl=if words.len()==4{parse_duration(words[3])?}else{ms(2000)};
                let expiry=self.now().checked_add(ttl).ok_or("time overflow")?;
                self.observe(id,s,expiry)?;self.admit()?;self.advance_to(self.now())?;mutation=true;self.status()
            },
            ["fault",id,op]=>{
                if !SENSORS.contains(id)||!matches!(*op,"drop"|"restore"){return Err("fault requires a sensor ID and drop|restore".into());}
                if *op=="drop"{self.dropped.insert((*id).into());self.observe(id,Status::Unknown,self.now()+ms(2000))?;}else{self.dropped.remove(*id);self.override_status.remove(*id);self.observe(id,self.truth.sample(id),self.now()+ms(2000))?;}
                self.log("InteractiveFault",line);self.admit()?;self.advance_to(self.now())?;mutation=true;self.status()
            },
            ["ack",a,op]=>{if !ACTIONS.contains(a)||!matches!(*op,"drop"|"restore"){return Err("ack requires an action name and drop|restore".into());}
                if *op=="drop"{self.ack_dropped.insert((*a).into());}else{self.ack_dropped.remove(*a);}self.log("AcknowledgementFault",line);mutation=true;self.status()},
            ["plant", field, value] => {
                let old = self.truth.clone();
                match (*field, *value) {
                    ("occupancy", "occupied") => self.truth.occupied[1] = true,
                    ("occupancy", "clear") => self.truth.occupied[1] = false,
                    ("egress", "blocked") => self.truth.egress_clear = false,
                    ("egress", "clear") => self.truth.egress_clear = true,
                    ("hazard", "on") => self.truth.hazard = [true;3],
                    ("hazard", "off") => self.truth.hazard = [false;3],
                    ("pressure", value) => {
                        let pressure:i32 = value.parse().map_err(|_| "pressure must be an integer")?;
                        if !(0..=200).contains(&pressure) { return Err("pressure must be between 0 and 200".into()); }
                        self.truth.pressure = pressure;
                    },
                    _ => return Err("plant occupancy occupied|clear, egress blocked|clear, hazard on|off, or pressure 0..200".into()),
                }
                self.log("PlantChanged", format!("old={old:?} new={:?}",self.truth));
                for v in self.truth.violations() { self.log("InvariantViolation",v); }
                // Truth changes do not immediately become observations.
                mutation=true; self.status()
            },
            ["replan"]=>{self.replan()?;self.admit()?;self.advance_to(self.now())?;mutation=true;self.status()},
            _=>return Err("unknown command or arguments; use help".into()),
        };
        if mutation {
            self.journal.push(line.into());
        }
        Ok(result)
    }
    fn recent_events(&self, n: usize) -> String {
        self.events
            .iter()
            .skip(self.events.len().saturating_sub(n))
            .map(|e| format!("{} {:?} {} {}", e.sequence, e.at, e.kind, e.detail))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
pub fn parse_duration(s: &str) -> Result<Duration> {
    let (n, mult) = if let Some(n) = s.strip_suffix("ms") {
        (n, 1)
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1000)
    } else {
        return Err("use an integer duration such as 500ms or 3s".into());
    };
    let n: u64 = n.parse().map_err(|_| "invalid duration")?;
    Ok(ms(n.checked_mul(mult).ok_or("duration overflow")?))
}
