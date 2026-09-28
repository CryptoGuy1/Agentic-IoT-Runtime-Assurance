use super::types::*;
use crate::evidence::{AssuranceStatus as Status, EvidenceAtom};
use crate::graph::{AssuranceGraph, NodeKind};
use crate::runtime::{AssuranceRuntime, AuditEvent, EvaluationMode};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

/// Owns all mutable execution authority. No mutable access to its inner kernel,
/// and no actuator callbacks occur during lease validation/local enqueueing.
#[derive(Debug)]
pub struct ActionRuntime {
    assurance: AssuranceRuntime,
    definitions: BTreeMap<String, ActionDefinition>,
    plans: BTreeMap<String, Plan>,
    instances: BTreeMap<String, ActionInstance>,
    leases: BTreeMap<String, AssuranceLease>,
    commands: BTreeMap<String, ActuatorCommand>,
    pending: VecDeque<String>,
    reserved: BTreeMap<String, String>,
    run_by_root: BTreeMap<String, BTreeSet<String>>,
    run_by_evidence: BTreeMap<String, BTreeSet<String>>,
    decisions: Vec<ActionDecision>,
    audit: Vec<ActionAuditEntry>,
}

impl ActionRuntime {
    pub fn new(
        graph: AssuranceGraph,
        definitions: Vec<ActionDefinition>,
    ) -> Result<Self, ActionError> {
        Self::with_mode(graph, definitions, EvaluationMode::Incremental)
    }
    pub fn with_mode(
        graph: AssuranceGraph,
        definitions: Vec<ActionDefinition>,
        mode: EvaluationMode,
    ) -> Result<Self, ActionError> {
        let mut defs = BTreeMap::new();
        let mut reserved = BTreeMap::new();
        for def in definitions {
            Self::validate_definition(&graph, &def)?;
            if defs.contains_key(&def.action_id) {
                return Err(ActionError::DuplicateIdentity);
            }
            for id in def.outcomes.ids() {
                if reserved.insert(id, def.action_id.clone()).is_some() {
                    return Err(ActionError::Configuration(
                        "outcome evidence belongs to more than one definition".into(),
                    ));
                }
            }
            defs.insert(def.action_id.clone(), def);
        }
        Ok(Self {
            assurance: AssuranceRuntime::with_mode(graph, mode),
            definitions: defs,
            plans: BTreeMap::new(),
            instances: BTreeMap::new(),
            leases: BTreeMap::new(),
            commands: BTreeMap::new(),
            pending: VecDeque::new(),
            reserved,
            run_by_root: BTreeMap::new(),
            run_by_evidence: BTreeMap::new(),
            decisions: Vec::new(),
            audit: Vec::new(),
        })
    }
    fn ancestors(graph: &AssuranceGraph, root: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![root.to_owned()];
        while let Some(id) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(rules) = graph.justifications_by_conclusion().get(&id) {
                for rule in rules {
                    stack.extend(graph.justifications()[rule].premises.iter().cloned());
                }
            }
        }
        seen
    }
    fn validate_definition(
        graph: &AssuranceGraph,
        d: &ActionDefinition,
    ) -> Result<(), ActionError> {
        let bad = |message: &str| ActionError::Configuration(format!("{}: {message}", d.action_id));
        if d.action_id.trim().is_empty()
            || d.max_start_lease.is_zero()
            || d.max_duration.is_zero()
            || d.min_duration > d.max_duration
            || d.outcomes.max_age.is_zero()
        {
            return Err(bad(
                "invalid identity, duration bounds or lease/outcome lifetime",
            ));
        }
        for (root, kind) in [
            (Some(&d.start_contract), NodeKind::ActionStart),
            (d.run_contract.as_ref(), NodeKind::ActionRun),
            (d.commit_contract.as_ref(), NodeKind::ActionCommit),
            (Some(&d.outcome_contract), NodeKind::ActionOutcome),
        ] {
            if let Some(root) = root
                && graph.nodes().get(root) != Some(&kind)
            {
                return Err(bad("contract root missing or has wrong node kind"));
            }
        }
        let caps = d.controllers;
        match d.interruptibility {
            Interruptibility::Preemptible if !caps.safe_abort => {
                return Err(bad("safe-abort controller required"));
            }
            Interruptibility::CompletionSafe if !caps.certified_complete => {
                return Err(bad("completion controller required"));
            }
            Interruptibility::Irreversible
                if d.commit_contract.is_none()
                    || !caps.safe_abort
                    || !caps.forward_mitigation
                    || d.recovery_type != RecoveryType::Irreversible =>
            {
                return Err(bad(
                    "irreversible actions require commit, precommit abort and mitigation",
                ));
            }
            _ => {}
        }
        if d.interruptibility != Interruptibility::Irreversible
            && d.recovery_type == RecoveryType::Irreversible
        {
            return Err(bad(
                "irreversible recovery requires irreversible action class",
            ));
        }
        if d.commit_contract.is_some() && !caps.safe_abort {
            return Err(bad("precommit abort controller required"));
        }
        if d.recovery_type == RecoveryType::Compensatable && !caps.compensate {
            return Err(bad("compensation controller required"));
        }
        let bindings = d.outcomes.ids();
        if d.outcomes.observed.is_empty()
            || bindings.len() != d.outcomes.observed.len() + 1
            || d.outcomes
                .observed
                .keys()
                .any(|key| key.trim().is_empty() || key == "acknowledged")
        {
            return Err(bad(
                "outcome bindings need distinct acknowledgement and observed evidence",
            ));
        }
        let ancestors = Self::ancestors(graph, &d.outcome_contract);
        for id in &bindings {
            if graph.nodes().get(id) != Some(&NodeKind::Evidence) || !ancestors.contains(id) {
                return Err(bad(
                    "outcome binding must be an evidence ancestor of the outcome root",
                ));
            }
        }
        for root in std::iter::once(&d.start_contract)
            .chain(d.run_contract.iter())
            .chain(d.commit_contract.iter())
        {
            if !Self::ancestors(graph, root).is_disjoint(&bindings) {
                return Err(bad(
                    "admission/run/commit cannot depend on this action's resettable outputs",
                ));
            }
        }
        Ok(())
    }
    pub fn now(&self) -> Duration {
        self.assurance.now()
    }
    pub fn assurance(&self) -> &AssuranceRuntime {
        &self.assurance
    }
    pub fn definitions(&self) -> &BTreeMap<String, ActionDefinition> {
        &self.definitions
    }
    pub fn plans(&self) -> &BTreeMap<String, Plan> {
        &self.plans
    }
    pub fn instances(&self) -> &BTreeMap<String, ActionInstance> {
        &self.instances
    }
    pub fn leases(&self) -> &BTreeMap<String, AssuranceLease> {
        &self.leases
    }
    pub fn commands(&self) -> &BTreeMap<String, ActuatorCommand> {
        &self.commands
    }
    pub fn decisions(&self) -> &[ActionDecision] {
        &self.decisions
    }
    pub fn audit(&self) -> &[ActionAuditEntry] {
        &self.audit
    }
    pub fn running_by_root(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.run_by_root
    }
    pub fn running_by_evidence(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.run_by_evidence
    }
    fn record(&mut self, event: ActionEvent) {
        self.audit.push(ActionAuditEntry {
            at: self.now(),
            epoch: self.assurance.epoch(),
            event,
        });
    }
    fn decide(
        &mut self,
        id: &str,
        phase: Option<ActionPhase>,
        kind: DecisionKind,
        status: Option<Status>,
    ) {
        let decision = ActionDecision {
            instance_id: id.into(),
            phase,
            kind,
            status,
        };
        self.decisions.push(decision.clone());
        self.record(ActionEvent::Decision(decision));
    }
    fn definition(&self, id: &str) -> &ActionDefinition {
        &self.definitions[&self.instances[id].action_id]
    }
    fn status(&self, root: &str) -> Status {
        self.assurance.evaluation().assurance_by_node[root].status()
    }
    fn current_plan(&self, id: &str) -> bool {
        let instance = &self.instances[id];
        self.plans[&instance.plan_id].version == instance.plan_version
    }
    fn predecessors_complete(&self, id: &str) -> bool {
        self.plans[&self.instances[id].plan_id]
            .precedence
            .iter()
            .filter(|(_, to)| to == id)
            .all(|(from, _)| self.instances[from].state == ActionState::Completed)
    }
    fn transition(&mut self, id: &str, next: ActionState) {
        use ActionState::*;
        let previous = self.instances[id].state;
        if previous == next {
            return;
        }
        let allowed = matches!(
            (previous, next),
            (Pending, Ready | Cancelled)
                | (Ready, Pending | Leased | Cancelled)
                | (Leased, Ready | Pending | Executing | Cancelled)
                | (
                    Executing,
                    Committed | Completed | Aborting | Recovering | Failed
                )
                | (Committed, Completed | Aborting | Recovering | Failed)
                | (Aborting, Recovering | Cancelled | Failed)
                | (Recovering, Completed | Cancelled | Failed)
        );
        assert!(
            allowed,
            "internal illegal lifecycle transition: {previous:?} -> {next:?}"
        );
        let now = self.now();
        let instance = self.instances.get_mut(id).unwrap();
        instance.state = next;
        if next == Executing {
            instance.started_at = Some(now);
        }
        if next == Committed {
            instance.committed_at = Some(now);
        }
        if next.is_terminal() {
            instance.completed_at = Some(now);
        }
        self.record(ActionEvent::StateChanged {
            instance_id: id.into(),
            previous,
            current: next,
        });
        if matches!(next, Failed | Cancelled) && self.instances[id].started_at.is_some() {
            let atoms = self
                .outcome_atoms(id, None)
                .expect("owned outcome counters remain representable");
            self.assurance
                .update_batch(atoms)
                .expect("owned outcome metadata was validated at configuration");
        }
    }
    pub fn register_plan(&mut self, mut plan: Plan) -> Result<(), ActionError> {
        if plan.plan_id.trim().is_empty() || self.plans.contains_key(&plan.plan_id) {
            return Err(ActionError::DuplicateIdentity);
        }
        for (id, action) in &plan.actions {
            if id.trim().is_empty() || self.instances.contains_key(id) {
                return Err(ActionError::DuplicateIdentity);
            }
            if !self.definitions.contains_key(action) {
                return Err(ActionError::Configuration(
                    "unknown action definition".into(),
                ));
            }
        }
        let mut remaining: BTreeSet<_> = plan.actions.keys().cloned().collect();
        for (from, to) in &plan.precedence {
            if from == to || !remaining.contains(from) || !remaining.contains(to) {
                return Err(ActionError::Configuration("invalid plan dependency".into()));
            }
        }
        while !remaining.is_empty() {
            let ready: Vec<_> = remaining
                .iter()
                .filter(|id| {
                    !plan
                        .precedence
                        .iter()
                        .any(|(from, to)| to == *id && remaining.contains(from))
                })
                .cloned()
                .collect();
            if ready.is_empty() {
                return Err(ActionError::Configuration("cyclic plan".into()));
            }
            for id in ready {
                remaining.remove(&id);
            }
        }
        plan.created_at = self.now();
        for (id, action) in &plan.actions {
            self.instances.insert(
                id.clone(),
                ActionInstance {
                    instance_id: id.clone(),
                    action_id: action.clone(),
                    plan_id: plan.plan_id.clone(),
                    plan_version: plan.version,
                    state: ActionState::Pending,
                    created_at: self.now(),
                    started_at: None,
                    committed_at: None,
                    completed_at: None,
                    active_lease_id: None,
                    active_command_id: None,
                },
            );
        }
        self.record(ActionEvent::PlanRegistered {
            plan_id: plan.plan_id.clone(),
            version: plan.version,
        });
        self.plans.insert(plan.plan_id.clone(), plan);
        self.refresh_ready();
        Ok(())
    }
    pub fn change_plan_version(&mut self, plan_id: &str, version: u64) -> Result<(), ActionError> {
        let old = self
            .plans
            .get(plan_id)
            .ok_or(ActionError::UnknownPlan)?
            .version;
        if version <= old {
            return Err(ActionError::PlanVersion);
        }
        let offset = self.assurance.audit().len();
        self.plans.get_mut(plan_id).unwrap().version = version;
        self.record(ActionEvent::PlanVersionChanged {
            plan_id: plan_id.into(),
            previous: old,
            current: version,
        });
        let ids: Vec<_> = self
            .instances
            .values()
            .filter(|i| i.plan_id == plan_id && !i.state.is_terminal())
            .map(|i| i.instance_id.clone())
            .collect();
        for id in ids {
            self.revoke_instance(&id, LeaseRevocation::PlanChanged);
            let state = self.instances[&id].state;
            if matches!(
                state,
                ActionState::Pending | ActionState::Ready | ActionState::Leased
            ) {
                self.transition(&id, ActionState::Cancelled);
            } else if state == ActionState::Executing
                && self.definition(&id).commit_contract.is_some()
            {
                self.begin_recovery(&id, Some(Status::Invalid), true);
            }
        }
        self.react(offset);
        Ok(())
    }
    fn root(&self, id: &str, phase: ActionPhase) -> Result<&str, ActionError> {
        let def = self.definition(id);
        match phase {
            ActionPhase::Start => Ok(&def.start_contract),
            ActionPhase::Commit => def
                .commit_contract
                .as_deref()
                .ok_or(ActionError::IllegalTransition),
        }
    }
    fn require_valid(
        &mut self,
        id: &str,
        phase: ActionPhase,
        root: &str,
    ) -> Result<(), ActionError> {
        let status = self.status(root);
        if status != Status::Valid {
            self.decide(
                id,
                Some(phase),
                if status == Status::Unknown {
                    DecisionKind::Revalidate
                } else {
                    DecisionKind::BlockAndReplan
                },
                Some(status),
            );
            return Err(ActionError::ContractUnavailable(status));
        }
        Ok(())
    }
    fn check_admission(&mut self, id: &str, phase: ActionPhase) -> Result<(), ActionError> {
        let instance = self.instances.get(id).ok_or(ActionError::UnknownInstance)?;
        if !self.current_plan(id) {
            return Err(ActionError::PlanVersion);
        }
        let state = instance.state;
        match phase {
            ActionPhase::Start => {
                if !matches!(
                    state,
                    ActionState::Pending | ActionState::Ready | ActionState::Leased
                ) {
                    return Err(ActionError::IllegalTransition);
                }
                if !self.predecessors_complete(id) {
                    return Err(ActionError::DependenciesIncomplete);
                }
                let bindings = self.definition(id).outcomes.ids();
                if self.instances.values().any(|other| {
                    other.instance_id != id
                        && other.state.occupies_bindings()
                        && !self.definitions[&other.action_id]
                            .outcomes
                            .ids()
                            .is_disjoint(&bindings)
                }) {
                    return Err(ActionError::BindingsBusy);
                }
            }
            ActionPhase::Commit => {
                if state != ActionState::Executing {
                    return Err(ActionError::IllegalTransition);
                }
            }
        }
        if phase == ActionPhase::Commit {
            let acknowledged = self.instances[id]
                .active_command_id
                .as_ref()
                .and_then(|id| self.commands.get(id))
                .is_some_and(|command| {
                    command.kind == CommandKind::Start
                        && command.delivered
                        && command.last_acknowledged == Some(true)
                });
            if !acknowledged {
                self.decide(
                    id,
                    Some(phase),
                    DecisionKind::Revalidate,
                    Some(Status::Unknown),
                );
                return Err(ActionError::StartNotAcknowledged);
            }
        }
        let root = self.root(id, phase)?.to_owned();
        self.require_valid(id, phase, &root)?;
        if let Some(run) = self.definition(id).run_contract.clone() {
            self.require_valid(id, phase, &run)?;
        }
        Ok(())
    }
    pub fn request_lease(
        &mut self,
        id: &str,
        phase: ActionPhase,
    ) -> Result<LeaseToken, ActionError> {
        self.check_admission(id, phase)?;
        let root = self.root(id, phase)?.to_owned();
        let witness = self.assurance.evaluation().preferred_witness_by_node[&root].clone();
        let cap = self
            .now()
            .checked_add(self.definition(id).max_start_lease)
            .ok_or(ActionError::TimeOverflow)?;
        // Preflight the execution deadline before issuing authority.
        if phase == ActionPhase::Start {
            self.now()
                .checked_add(self.definition(id).max_duration)
                .ok_or(ActionError::TimeOverflow)?;
        }
        self.revoke_instance(id, LeaseRevocation::Replaced);
        let lease_id = format!("lease-{}", self.leases.len());
        let nonce = format!("nonce-{}", self.leases.len());
        let evidence_versions = witness
            .evidence_ids
            .iter()
            .map(|e| (e.clone(), self.assurance.evidence()[e].version))
            .collect();
        self.leases.insert(
            lease_id.clone(),
            AssuranceLease {
                lease_id: lease_id.clone(),
                action_instance_id: id.into(),
                phase,
                plan_version: self.instances[id].plan_version,
                assurance_epoch: self.assurance.epoch(),
                issued_at: self.now(),
                expires_at: cap.min(witness.horizon),
                witness,
                evidence_versions,
                nonce: nonce.clone(),
                consumed: false,
                revoked: None,
            },
        );
        if phase == ActionPhase::Start {
            self.transition(id, ActionState::Leased);
        }
        self.instances.get_mut(id).unwrap().active_lease_id = Some(lease_id.clone());
        self.record(ActionEvent::LeaseIssued {
            lease_id: lease_id.clone(),
        });
        self.decide(id, Some(phase), DecisionKind::Eligible, Some(Status::Valid));
        Ok(LeaseToken { lease_id, nonce })
    }
    fn lease_problem(&self, lease: &AssuranceLease) -> Option<LeaseRevocation> {
        if !self.current_plan(&lease.action_instance_id) {
            return Some(LeaseRevocation::PlanChanged);
        }
        if self.now() >= lease.expires_at {
            return Some(LeaseRevocation::Expired);
        }
        if lease.evidence_versions.iter().any(|(id, version)| {
            self.assurance.evidence().get(id).is_none_or(|a| {
                a.version != *version || a.value_at(self.now()).status() != Status::Valid
            })
        }) {
            return Some(LeaseRevocation::EvidenceChanged);
        }
        if self.root(&lease.action_instance_id, lease.phase).is_err() {
            return Some(LeaseRevocation::ContractChanged);
        }
        if self.status(self.root(&lease.action_instance_id, lease.phase).unwrap()) != Status::Valid
        {
            return Some(LeaseRevocation::ContractChanged);
        }
        None
    }
    fn revoke(&mut self, lease_id: &str, reason: LeaseRevocation) {
        let lease = self.leases.get_mut(lease_id).unwrap();
        if lease.consumed || lease.revoked.is_some() {
            return;
        }
        lease.revoked = Some(reason);
        let id = lease.action_instance_id.clone();
        if self.instances[&id].active_lease_id.as_deref() == Some(lease_id) {
            self.instances.get_mut(&id).unwrap().active_lease_id = None;
            if self.instances[&id].state == ActionState::Leased {
                self.transition(&id, ActionState::Ready);
            }
        }
        self.record(ActionEvent::LeaseRevoked {
            lease_id: lease_id.into(),
            reason,
        });
    }
    fn revoke_instance(&mut self, id: &str, reason: LeaseRevocation) {
        if let Some(lease) = self.instances[id].active_lease_id.clone() {
            self.revoke(&lease, reason);
        }
    }
    pub fn dispatch(
        &mut self,
        id: &str,
        phase: ActionPhase,
        token: &LeaseToken,
    ) -> Result<String, ActionError> {
        let result = self.dispatch_inner(id, phase, token);
        if result.is_err() && self.instances.contains_key(id) {
            self.decide(id, Some(phase), DecisionKind::LeaseRejected, None);
        }
        result
    }
    fn dispatch_inner(
        &mut self,
        id: &str,
        phase: ActionPhase,
        token: &LeaseToken,
    ) -> Result<String, ActionError> {
        let lease = self
            .leases
            .get(&token.lease_id)
            .ok_or(ActionError::UnknownLease)?;
        if lease.action_instance_id != id || lease.phase != phase || lease.nonce != token.nonce {
            return Err(ActionError::WrongLease);
        }
        if lease.consumed {
            return Err(ActionError::ConsumedLease);
        }
        if let Some(reason) = lease.revoked {
            return Err(ActionError::RevokedLease(reason));
        }
        if let Some(reason) = self.lease_problem(lease) {
            self.revoke(&token.lease_id, reason);
            self.refresh_ready();
            return Err(ActionError::RevokedLease(reason));
        }
        self.check_admission(id, phase)?;
        if self.instances[id].active_lease_id.as_deref() != Some(&token.lease_id) {
            return Err(ActionError::WrongLease);
        }
        if phase == ActionPhase::Start {
            self.now()
                .checked_add(self.definition(id).max_duration)
                .ok_or(ActionError::TimeOverflow)?;
        }
        let atoms = self.outcome_atoms(id, None)?;
        let offset = self.assurance.audit().len();
        self.assurance.update_batch(atoms)?;
        // There is no external callback or await from validation through enqueue.
        self.leases.get_mut(&token.lease_id).unwrap().consumed = true;
        self.instances.get_mut(id).unwrap().active_lease_id = None;
        self.record(ActionEvent::LeaseConsumed {
            lease_id: token.lease_id.clone(),
        });
        self.transition(
            id,
            if phase == ActionPhase::Start {
                ActionState::Executing
            } else {
                ActionState::Committed
            },
        );
        let command = self.enqueue(
            id,
            if phase == ActionPhase::Start {
                CommandKind::Start
            } else {
                CommandKind::Commit
            },
        );
        self.rebuild_subscriptions();
        self.react(offset);
        Ok(command)
    }
    fn enqueue(&mut self, id: &str, kind: CommandKind) -> String {
        let command_id = format!("command-{}", self.commands.len());
        self.commands.insert(
            command_id.clone(),
            ActuatorCommand {
                command_id: command_id.clone(),
                instance_id: id.into(),
                kind,
                issued_at: self.now(),
                delivered: false,
                last_result_sequence: None,
                last_observed_at: None,
                completion_reported: false,
                last_acknowledged: None,
            },
        );
        self.pending.push_back(command_id.clone());
        self.instances.get_mut(id).unwrap().active_command_id = Some(command_id.clone());
        self.record(ActionEvent::CommandQueued {
            command_id: command_id.clone(),
        });
        command_id
    }
    /// Delivers each queued command at most once; superseded commands are skipped.
    pub fn take_command(&mut self) -> Option<ActuatorCommand> {
        while let Some(id) = self.pending.pop_front() {
            let command = &self.commands[&id];
            let instance = &self.instances[&command.instance_id];
            if instance.state.is_terminal() || instance.active_command_id.as_deref() != Some(&id) {
                continue;
            }
            self.commands.get_mut(&id).unwrap().delivered = true;
            self.record(ActionEvent::CommandDelivered {
                command_id: id.clone(),
            });
            return Some(self.commands[&id].clone());
        }
        None
    }
    fn outcome_atoms(
        &self,
        id: &str,
        result: Option<&ActuatorResult>,
    ) -> Result<Vec<EvidenceAtom>, ActionError> {
        let def = self.definition(id);
        let observed = result.map_or(self.now(), |r| r.observed_at);
        let ack = result
            .and_then(|r| r.acknowledged)
            .map_or(Status::Unknown, |a| {
                if a { Status::Valid } else { Status::Invalid }
            });
        let fields = std::iter::once(("acknowledged", def.outcomes.acknowledgement.as_str(), ack))
            .chain(def.outcomes.observed.iter().map(|(key, e)| {
                (
                    key.as_str(),
                    e.as_str(),
                    result
                        .and_then(|r| r.observed_state.get(key))
                        .copied()
                        .unwrap_or(Status::Unknown),
                )
            }));
        fields
            .map(|(key, e, status)| {
                let version = self
                    .assurance
                    .evidence()
                    .get(e)
                    .map_or(Some(0), |a| a.version.checked_add(1))
                    .ok_or(ActionError::InvalidResult)?;
                Ok(EvidenceAtom {
                    evidence_id: e.into(),
                    evidence_type: "actuator".into(),
                    predicate: key.into(),
                    source_id: format!("actuator:{}", def.action_id),
                    version,
                    observed_at: observed,
                    expires_at: observed.saturating_add(def.outcomes.max_age),
                    status,
                    payload_hash: None,
                })
            })
            .collect()
    }
    pub fn submit_result(&mut self, result: ActuatorResult) -> Result<(), ActionError> {
        let command = self
            .commands
            .get(&result.command_id)
            .ok_or(ActionError::UnknownCommand)?
            .clone();
        let id = command.instance_id.clone();
        if self.instances[&id].state.is_terminal()
            || self.instances[&id].active_command_id.as_deref() != Some(&result.command_id)
        {
            return Err(ActionError::SupersededCommand);
        }
        if !command.delivered {
            return Err(ActionError::UndeliveredCommand);
        }
        if command
            .last_result_sequence
            .is_some_and(|seq| result.sequence <= seq)
            || command
                .last_observed_at
                .is_some_and(|at| result.observed_at < at)
        {
            return Err(ActionError::StaleResult);
        }
        if result.observed_at > self.now() || result.observed_at < command.issued_at {
            return Err(ActionError::InvalidResult);
        }
        let normal = matches!(
            command.kind,
            CommandKind::Start | CommandKind::Commit | CommandKind::CertifiedComplete
        );
        if (!normal && !result.observed_state.is_empty())
            || result
                .observed_state
                .keys()
                .any(|key| !self.definition(&id).outcomes.observed.contains_key(key))
        {
            return Err(ActionError::InvalidResult);
        }
        let offset = self.assurance.audit().len();
        if normal {
            let atoms = self.outcome_atoms(&id, Some(&result))?;
            self.assurance.update_batch(atoms)?;
        }
        let stored = self.commands.get_mut(&result.command_id).unwrap();
        stored.last_acknowledged = result.acknowledged;
        stored.last_result_sequence = Some(result.sequence);
        stored.last_observed_at = Some(result.observed_at);
        stored.completion_reported = result.completed && result.acknowledged == Some(true);
        self.record(ActionEvent::ResultAccepted {
            command_id: result.command_id.clone(),
            sequence: result.sequence,
        });
        if result.acknowledged == Some(false) {
            if self.instances[&id].state.is_running() {
                self.begin_recovery(&id, Some(Status::Invalid), false);
            } else {
                self.transition(&id, ActionState::Failed);
            }
        } else if stored_success(&result) && !normal {
            if command.kind == CommandKind::SafeAbort
                && self.definition(&id).recovery_type == RecoveryType::Compensatable
            {
                self.transition(&id, ActionState::Recovering);
                self.enqueue(&id, CommandKind::Recover);
                self.decide(
                    &id,
                    None,
                    DecisionKind::RecoveryRequested(CommandKind::Recover),
                    None,
                );
            } else {
                self.transition(
                    &id,
                    if command.kind == CommandKind::ForwardMitigation {
                        ActionState::Failed
                    } else {
                        ActionState::Cancelled
                    },
                );
            }
        } else if stored_success(&result)
            && self.status(&self.definition(&id).outcome_contract) != Status::Valid
        {
            let status = self.status(&self.definition(&id).outcome_contract);
            self.decide(
                &id,
                None,
                if status == Status::Unknown {
                    DecisionKind::Revalidate
                } else {
                    DecisionKind::BlockAndReplan
                },
                Some(status),
            );
        }
        self.react(offset);
        Ok(())
    }
    pub fn update(&mut self, atom: EvidenceAtom) -> Result<(), ActionError> {
        if self.reserved.contains_key(&atom.evidence_id) {
            return Err(ActionError::ReservedEvidence);
        }
        let offset = self.assurance.audit().len();
        self.assurance.update(atom)?;
        self.react(offset);
        Ok(())
    }
    pub fn advance(&mut self, elapsed: Duration) -> Result<(), ActionError> {
        self.advance_to(
            self.now()
                .checked_add(elapsed)
                .ok_or(ActionError::TimeOverflow)?,
        )
    }
    pub fn advance_to(&mut self, target: Duration) -> Result<(), ActionError> {
        if target < self.now() {
            return Err(crate::runtime::RuntimeError::TimeWentBackwards.into());
        }
        while self.now() < target {
            let mut next = target;
            for atom in self
                .assurance
                .evidence()
                .values()
                .filter(|a| a.status == Status::Valid && a.expires_at > self.now())
            {
                next = next.min(atom.expires_at);
            }
            for lease in self
                .leases
                .values()
                .filter(|l| !l.consumed && l.revoked.is_none() && l.expires_at > self.now())
            {
                next = next.min(lease.expires_at);
            }
            for instance in self.instances.values().filter(|i| {
                i.state.is_running()
                    || matches!(i.state, ActionState::Aborting | ActionState::Recovering)
            }) {
                let def = &self.definitions[&instance.action_id];
                if let Some(start) = instance.started_at {
                    let min = start.saturating_add(def.min_duration);
                    if min > self.now() {
                        next = next.min(min);
                    }
                    if instance.state.is_running() {
                        let max = start.saturating_add(def.max_duration);
                        if max > self.now() {
                            next = next.min(max);
                        }
                    }
                }
                if matches!(
                    instance.state,
                    ActionState::Aborting | ActionState::Recovering
                ) {
                    let command = &self.commands[instance.active_command_id.as_ref().unwrap()];
                    let deadline = command.issued_at.saturating_add(def.max_duration);
                    if deadline > self.now() {
                        next = next.min(deadline);
                    }
                }
            }
            let offset = self.assurance.audit().len();
            self.assurance.advance_to(next)?;
            self.react(offset);
        }
        Ok(())
    }
    fn refresh_ready(&mut self) {
        let ids: Vec<_> = self
            .instances
            .values()
            .filter(|i| {
                matches!(
                    i.state,
                    ActionState::Pending | ActionState::Ready | ActionState::Leased
                )
            })
            .map(|i| i.instance_id.clone())
            .collect();
        for id in ids {
            let ready = self.current_plan(&id)
                && self.predecessors_complete(&id)
                && self.status(&self.definition(&id).start_contract) == Status::Valid;
            if !ready {
                self.revoke_instance(&id, LeaseRevocation::ContractChanged);
                self.transition(&id, ActionState::Pending);
            } else if self.instances[&id].state == ActionState::Pending {
                self.transition(&id, ActionState::Ready);
            }
        }
    }
    fn begin_recovery(&mut self, id: &str, trigger: Option<Status>, precommit_cancel: bool) {
        if !self.instances[id].state.is_running() {
            return;
        }
        self.revoke_instance(id, LeaseRevocation::ActionStopped);
        let def = self.definition(id);
        let kind = if precommit_cancel
            || (def.commit_contract.is_some() && self.instances[id].committed_at.is_none())
        {
            CommandKind::SafeAbort
        } else {
            match def.interruptibility {
                Interruptibility::Preemptible => CommandKind::SafeAbort,
                Interruptibility::CompletionSafe => CommandKind::CertifiedComplete,
                Interruptibility::Irreversible
                    if self.instances[id].state == ActionState::Committed =>
                {
                    CommandKind::ForwardMitigation
                }
                Interruptibility::Irreversible => CommandKind::SafeAbort,
            }
        };
        self.transition(
            id,
            if kind == CommandKind::SafeAbort {
                ActionState::Aborting
            } else {
                ActionState::Recovering
            },
        );
        // These IDs/versions are exclusively owned by this runtime. Resetting
        // support is not a claim that a physical recovery has succeeded.
        let atoms = self
            .outcome_atoms(id, None)
            .expect("owned outcome counters remain representable");
        self.assurance
            .update_batch(atoms)
            .expect("owned outcome metadata was validated at configuration");
        self.enqueue(id, kind);
        self.decide(id, None, DecisionKind::RecoveryRequested(kind), trigger);
    }
    fn try_complete(&mut self, id: &str) {
        let instance = &self.instances[id];
        let Some(command_id) = &instance.active_command_id else {
            return;
        };
        let command = &self.commands[command_id];
        let def = self.definition(id);
        let allowed = (instance.state == ActionState::Executing && def.commit_contract.is_none())
            || instance.state == ActionState::Committed
            || (instance.state == ActionState::Recovering
                && command.kind == CommandKind::CertifiedComplete);
        if allowed
            && command.completion_reported
            && instance
                .started_at
                .is_some_and(|start| self.now() >= start.saturating_add(def.min_duration))
            && self.status(&def.outcome_contract) == Status::Valid
        {
            self.revoke_instance(id, LeaseRevocation::ActionStopped);
            self.transition(id, ActionState::Completed);
        }
    }
    fn check_running_support(&mut self, id: &str) {
        if self.instances[id].state.is_running()
            && let Some(root) = &self.definition(id).run_contract
        {
            let status = self.status(root);
            if status != Status::Valid {
                self.begin_recovery(id, Some(status), false);
            }
        }
    }
    fn react(&mut self, audit_offset: usize) {
        let mut cursor = audit_offset;
        loop {
            let leases: Vec<_> = self
                .leases
                .values()
                .filter(|l| !l.consumed && l.revoked.is_none())
                .filter_map(|l| {
                    self.lease_problem(l)
                        .map(|reason| (l.lease_id.clone(), reason))
                })
                .collect();
            for (id, reason) in leases {
                self.revoke(&id, reason);
            }
            let mut affected = BTreeSet::new();
            let batch_end = self.assurance.audit().len();
            for entry in &self.assurance.audit()[cursor..batch_end] {
                let (root, evidence) = match &entry.event {
                    AuditEvent::AssuranceChanged { node_id, .. }
                    | AuditEvent::WitnessChanged { node_id, .. } => (Some(node_id), None),
                    AuditEvent::EvidenceUpdated { current, .. } => {
                        (None, Some(&current.evidence_id))
                    }
                    AuditEvent::EvidenceExpired { evidence_id, .. } => (None, Some(evidence_id)),
                };
                if let Some(ids) = root.and_then(|r| self.run_by_root.get(r)) {
                    affected.extend(ids.iter().cloned());
                }
                if let Some(ids) = evidence.and_then(|e| self.run_by_evidence.get(e)) {
                    affected.extend(ids.iter().cloned());
                }
            }
            for id in affected {
                self.check_running_support(&id);
            }
            let active: Vec<_> = self
                .instances
                .values()
                .filter(|i| !i.state.is_terminal())
                .map(|i| i.instance_id.clone())
                .collect();
            for id in active {
                // A prior controller reaction in this batch may have invalidated
                // an upstream outcome. Never complete against newly lost run support.
                self.check_running_support(&id);
                self.try_complete(&id);
                let instance = &self.instances[&id];
                let max = self.definition(&id).max_duration;
                if instance.state.is_running()
                    && instance
                        .started_at
                        .is_some_and(|t| self.now() >= t.saturating_add(max))
                {
                    self.decide(&id, None, DecisionKind::Timeout, None);
                    self.begin_recovery(&id, None, false);
                } else if matches!(
                    instance.state,
                    ActionState::Aborting | ActionState::Recovering
                ) {
                    let command = &self.commands[instance.active_command_id.as_ref().unwrap()];
                    if self.now() >= command.issued_at.saturating_add(max) {
                        self.decide(&id, None, DecisionKind::Timeout, None);
                        self.transition(&id, ActionState::Failed);
                    }
                }
            }
            cursor = batch_end;
            if cursor == self.assurance.audit().len() {
                break;
            }
        }
        self.refresh_ready();
        self.rebuild_subscriptions();
    }
    fn rebuild_subscriptions(&mut self) {
        self.run_by_root.clear();
        self.run_by_evidence.clear();
        for instance in self.instances.values().filter(|i| i.state.is_running()) {
            if let Some(root) = &self.definitions[&instance.action_id].run_contract {
                self.run_by_root
                    .entry(root.clone())
                    .or_default()
                    .insert(instance.instance_id.clone());
                if let Some(witness) = self
                    .assurance
                    .evaluation()
                    .preferred_witness_by_node
                    .get(root)
                {
                    for evidence in &witness.evidence_ids {
                        self.run_by_evidence
                            .entry(evidence.clone())
                            .or_default()
                            .insert(instance.instance_id.clone());
                    }
                }
            }
        }
    }
}
fn stored_success(result: &ActuatorResult) -> bool {
    result.completed && result.acknowledged == Some(true)
}
