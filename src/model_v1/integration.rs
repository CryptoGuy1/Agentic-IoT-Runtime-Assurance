//! Optional model gate around the existing action runtime. All physical command
//! release must pass through this owner; the inner runtime is read-only.
use super::*;
use crate::actions::*;
use crate::evidence::{AssuranceStatus, EvidenceAtom};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ModelBinding {
    pub action: Action,
    /// Explicitly configured, permitted recovery command; never inferred as safe.
    pub recovery: Action,
    pub max_delay: Duration,
    pub may_drop: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ModelAudit {
    pub instance: String,
    pub recorded_at: Duration,
    pub phase: &'static str,
    pub check: CheckResult,
}
#[derive(Debug)]
pub enum IntegratedError {
    Runtime(ActionError),
    Configuration(String),
    Blocked(CheckResult),
}
impl From<ActionError> for IntegratedError {
    fn from(e: ActionError) -> Self {
        Self::Runtime(e)
    }
}
#[derive(Clone, Debug)]
pub struct PhysicalCommand {
    /// None for a separately authorized persistent-effect recovery.
    pub runtime_command: Option<ActuatorCommand>,
    pub model_command: Command,
}
#[derive(Debug)]
pub struct ModelRuntime {
    runtime: ActionRuntime,
    estimator: Estimator,
    bindings: BTreeMap<String, ModelBinding>,
    effects: BTreeMap<u8, (String, Action)>,
    held: Option<ActuatorCommand>,
    released: BTreeMap<String, Duration>,
    persistent_pending: BTreeSet<String>,
    audit: Vec<ModelAudit>,
    claim_configuration: Option<ClaimConfiguration>,
    certificates: Vec<PhysicalCertificate>,
    policy: RefinementPolicy,
    recovery_alternatives: BTreeMap<String, Vec<Action>>,
}
impl ModelRuntime {
    /// Bind every definition; attach only before plans/commands exist. Controller
    /// commands map to the declared recovery, not to an assumed successful abort.
    pub fn new(
        runtime: ActionRuntime,
        estimator: Estimator,
        bindings: BTreeMap<String, ModelBinding>,
    ) -> Result<Self, IntegratedError> {
        if runtime.now() != estimator.state().at
            || !estimator.commands().is_empty()
            || estimator.initial_inputs() != Inputs::default()
            || !runtime.plans().is_empty()
            || bindings.len() != runtime.definitions().len()
            || runtime
                .definitions()
                .keys()
                .any(|id| !bindings.contains_key(id))
            || bindings
                .values()
                .any(|b| !super::types::grid(b.max_delay) || b.max_delay > Duration::from_secs(60))
        {
            return Err(IntegratedError::Configuration("bind every definition before registering plans; use synchronized grid time and bounded delivery".into()));
        }
        Ok(Self {
            runtime,
            estimator,
            bindings,
            effects: BTreeMap::new(),
            held: None,
            released: BTreeMap::new(),
            persistent_pending: BTreeSet::new(),
            audit: vec![],
            claim_configuration: None,
            certificates: vec![],
            policy: RefinementPolicy::B6,
            recovery_alternatives: BTreeMap::new(),
        })
    }
    /// Attach reviewed, explicitly migrated physical claim nodes. Permissions and
    /// outcomes retain their existing graph requirements and producer ownership.
    pub fn with_claims(
        runtime: ActionRuntime,
        estimator: Estimator,
        bindings: BTreeMap<String, ModelBinding>,
        configuration: ClaimConfiguration,
    ) -> Result<Self, IntegratedError> {
        let definitions: Vec<_> = runtime.definitions().values().cloned().collect();
        super::claims::validate_mapping(
            runtime.assurance().graph(),
            &definitions,
            &configuration,
            true,
        )
        .map_err(IntegratedError::Configuration)?;
        if configuration.replacements.iter().any(|b| {
            runtime
                .assurance()
                .evidence()
                .contains_key(&b.model_evidence)
        }) {
            return Err(IntegratedError::Configuration(
                "model claim nodes must start without externally supplied evidence".into(),
            ));
        }
        let mut owner = Self::new(runtime, estimator, bindings)?;
        owner.claim_configuration = Some(configuration);
        owner.refresh_claims()?;
        Ok(owner)
    }
    pub fn certificates(&self) -> &[PhysicalCertificate] {
        &self.certificates
    }
    pub fn certificate(&self, evidence_id: &str) -> Option<&PhysicalCertificate> {
        self.certificates
            .iter()
            .rev()
            .find(|c| c.evidence_id == evidence_id)
    }
    fn refresh_claims(&mut self) -> Result<(), IntegratedError> {
        let Some(config) = &self.claim_configuration else {
            return Ok(());
        };
        if self
            .certificates
            .last()
            .is_some_and(|c| c.state_revision == self.estimator.revision())
        {
            return Ok(());
        }
        let roots: Vec<_> = self
            .runtime
            .definitions()
            .values()
            .flat_map(|d| {
                std::iter::once(&d.start_contract)
                    .chain(d.run_contract.iter())
                    .chain(d.commit_contract.iter())
            })
            .cloned()
            .collect();
        let graph = self.runtime.assurance().graph();
        let mut certificates = vec![];
        for binding in &config.replacements {
            let version = self
                .certificate(&binding.model_evidence)
                .map_or(Some(1), |c| c.version.checked_add(1))
                .ok_or_else(|| {
                    IntegratedError::Configuration("certificate version overflow".into())
                })?;
            let affected = roots
                .iter()
                .filter(|root| {
                    super::claims::ancestors(graph, root).contains(&binding.model_evidence)
                })
                .cloned()
                .collect();
            certificates.push(
                super::claims::produce(&self.estimator, config, binding, version, affected)
                    .map_err(IntegratedError::Configuration)?,
            );
        }
        self.runtime
            .update_model_claims(certificates.iter().map(PhysicalCertificate::atom).collect())?;
        self.certificates.extend(certificates);
        Ok(())
    }
    /// Comparison-only policy selection, confined to the experiment crate.
    pub(crate) fn comparison_policy(&mut self, policy: RefinementPolicy) {
        self.policy = policy;
    }
    /// Explicitly authorized ordered alternatives, frozen before any plan exists.
    pub fn configure_recoveries(
        &mut self,
        action_id: &str,
        alternatives: Vec<Action>,
    ) -> Result<(), IntegratedError> {
        if !self.runtime.plans().is_empty()
            || !self.bindings.contains_key(action_id)
            || alternatives.len() > 4
            || alternatives
                .iter()
                .enumerate()
                .any(|(i, a)| alternatives[..i].contains(a))
        {
            return Err(IntegratedError::Configuration(
                "configure up to four unique recovery alternatives before registering plans".into(),
            ));
        }
        self.recovery_alternatives
            .insert(action_id.into(), alternatives);
        Ok(())
    }
    fn choose_recovery(
        &mut self,
        id: &str,
        candidate: Option<Command>,
        phase: &'static str,
        release: bool,
    ) -> Result<Action, IntegratedError> {
        let b = self.binding(id)?;
        if self.policy != RefinementPolicy::B6 {
            return Ok(b.recovery);
        }
        let action_id = self.runtime.instances()[id].action_id.clone();
        let mut options = vec![b.recovery];
        if let Some(alternatives) = self.recovery_alternatives.get(&action_id) {
            for action in alternatives {
                if !options.contains(action) {
                    options.push(*action);
                }
            }
        }
        let mut last = None;
        for action in options {
            let check = if release {
                self.estimator
                    .check_recovery_delivery(action, b.max_delay, b.may_drop)
            } else {
                self.estimator
                    .check_recovery(candidate.clone(), action, b.max_delay, b.may_drop)
            }
            .map_err(IntegratedError::Configuration)?;
            let supported = check.decision == Decision::Supported;
            self.audit.push(ModelAudit {
                instance: id.into(),
                recorded_at: self.runtime.now(),
                phase,
                check: check.clone(),
            });
            if supported {
                return Ok(action);
            }
            last = Some(check);
        }
        Err(IntegratedError::Blocked(
            last.expect("primary recovery always exists"),
        ))
    }
    pub fn runtime(&self) -> &ActionRuntime {
        &self.runtime
    }
    pub fn estimator(&self) -> &Estimator {
        &self.estimator
    }
    pub fn audit(&self) -> &[ModelAudit] {
        &self.audit
    }
    pub fn register_plan(&mut self, plan: Plan) -> Result<(), IntegratedError> {
        self.runtime.register_plan(plan)?;
        Ok(())
    }
    pub fn update_permission(&mut self, atom: EvidenceAtom) -> Result<(), IntegratedError> {
        if self.claim_configuration.as_ref().is_some_and(|c| {
            c.replacements
                .iter()
                .any(|b| b.model_evidence == atom.evidence_id)
        }) {
            return Err(ActionError::ReservedEvidence.into());
        }
        self.runtime.update(atom)?;
        Ok(())
    }
    fn binding(&self, id: &str) -> Result<ModelBinding, IntegratedError> {
        let i = self
            .runtime
            .instances()
            .get(id)
            .ok_or(ActionError::UnknownInstance)?;
        Ok(self.bindings[&i.action_id].clone())
    }
    fn candidate(&self, action: Action, b: &ModelBinding) -> Result<Command, IntegratedError> {
        let id = self
            .estimator
            .commands()
            .last()
            .map_or(Some(1), |c| c.id.checked_add(1))
            .ok_or_else(|| IntegratedError::Configuration("command IDs exhausted".into()))?;
        Ok(Command {
            id,
            action,
            issued_at: self.runtime.now(),
            max_delay: b.max_delay,
            may_drop: b.may_drop,
        })
    }
    fn record_check(
        &mut self,
        id: &str,
        phase: &'static str,
        check: CheckResult,
    ) -> Result<(), IntegratedError> {
        self.audit.push(ModelAudit {
            instance: id.into(),
            recorded_at: self.runtime.now(),
            phase,
            check: check.clone(),
        });
        if check.decision != Decision::Supported {
            return Err(IntegratedError::Blocked(check));
        }
        Ok(())
    }
    fn admit(
        &mut self,
        id: &str,
        action: Action,
        recovery: bool,
    ) -> Result<Command, IntegratedError> {
        let b = self.binding(id)?;
        let candidate = self.candidate(action, &b)?;
        if self.policy != RefinementPolicy::B6 {
            if self.policy == RefinementPolicy::B3
                && !recovery
                && self.runtime.instances()[id].started_at.is_none()
            {
                let check = self.estimator.check_point_delivery(action, b.max_delay);
                self.record_check(id, "point-model-once", check)?;
            }
            return Ok(candidate);
        }
        let check = if recovery {
            self.estimator
                .check_recovery_delivery(action, b.max_delay, b.may_drop)
                .map_err(IntegratedError::Configuration)?
        } else {
            self.estimator
                .check_delivery(action, b.max_delay, b.may_drop)
        };
        self.record_check(
            id,
            if recovery {
                "recovery-release"
            } else {
                "candidate"
            },
            check,
        )?;
        if !recovery {
            self.choose_recovery(id, Some(candidate.clone()), "recovery-feasibility", false)?;
        }
        Ok(candidate)
    }
    pub fn request_lease(
        &mut self,
        id: &str,
        phase: ActionPhase,
    ) -> Result<LeaseToken, IntegratedError> {
        self.refresh_claims()?;
        self.admit(id, self.binding(id)?.action, false)?;
        Ok(self.runtime.request_lease(id, phase)?)
    }
    pub fn dispatch(
        &mut self,
        id: &str,
        phase: ActionPhase,
        token: &LeaseToken,
    ) -> Result<String, IntegratedError> {
        self.refresh_claims()?;
        self.admit(id, self.binding(id)?.action, false)?;
        Ok(self.runtime.dispatch(id, phase, token)?)
    }
    /// Local release boundary. Failed checks retain the command for inspection or
    /// retry; they do not mark any physical command as issued in the estimator.
    pub fn take_command(&mut self) -> Result<Option<PhysicalCommand>, IntegratedError> {
        if self.held.as_ref().is_some_and(|c| {
            self.runtime.instances()[&c.instance_id]
                .active_command_id
                .as_deref()
                != Some(&c.command_id)
                || self.runtime.instances()[&c.instance_id].state.is_terminal()
        }) {
            self.held = None;
        }
        if self.held.is_none() {
            self.held = self.runtime.take_command();
        }
        let Some(command) = self.held.clone() else {
            if let Some(id) = self.persistent_pending.first().cloned() {
                return self.recover_persistent(&id).map(Some);
            }
            return Ok(None);
        };
        let b = self.binding(&command.instance_id)?;
        let recovery = !matches!(command.kind, CommandKind::Start | CommandKind::Commit);
        if !recovery {
            let instance = &self.runtime.instances()[&command.instance_id];
            let def = &self.runtime.definitions()[&instance.action_id];
            let root = if command.kind == CommandKind::Commit {
                def.commit_contract.as_ref().unwrap()
            } else {
                &def.start_contract
            };
            for root in std::iter::once(root).chain(def.run_contract.iter()) {
                let status = self.runtime.assurance().evaluation().assurance_by_node[root].status();
                if status != AssuranceStatus::Valid {
                    return Err(ActionError::ContractUnavailable(status).into());
                }
            }
            if self.runtime.plans()[&instance.plan_id].version != instance.plan_version {
                return Err(ActionError::PlanVersion.into());
            }
        }
        // CertifiedComplete means completing the original physical action. Its
        // target must be feasible; it is not mapped to an opposite recovery.
        let action = if command.kind == CommandKind::CertifiedComplete {
            b.action
        } else if recovery {
            self.choose_recovery(&command.instance_id, None, "recovery-selection", true)?
        } else {
            b.action
        };
        let model_command = self.admit(&command.instance_id, action, recovery)?;
        self.estimator
            .note_command(model_command.clone())
            .map_err(IntegratedError::Configuration)?;
        self.effects
            .insert(action.device(), (command.instance_id.clone(), action));
        self.persistent_pending
            .retain(|owner| self.effects.values().any(|(id, _)| id == owner));
        self.released
            .insert(command.command_id.clone(), self.runtime.now());
        self.held = None;
        self.refresh_claims()?;
        Ok(Some(PhysicalCommand {
            runtime_command: Some(command),
            model_command,
        }))
    }
    pub fn submit_result(&mut self, result: ActuatorResult) -> Result<(), IntegratedError> {
        let Some(released_at) = self.released.get(&result.command_id) else {
            return Err(ActionError::UndeliveredCommand.into());
        };
        if result.observed_at < *released_at {
            return Err(ActionError::InvalidResult.into());
        }
        self.runtime.submit_result(result)?;
        Ok(())
    }
    pub fn observe(&mut self, observation: Observation) -> Result<Update, IntegratedError> {
        let update = self
            .estimator
            .observe(observation)
            .map_err(IntegratedError::Configuration)?;
        self.refresh_claims()?;
        self.monitor()?;
        Ok(update)
    }
    /// Visit every 100 ms boundary, even when the caller jumps far ahead. Inner
    /// lifecycle deadlines remain exact. Validate on a clone before mutating.
    pub fn advance_to(&mut self, at: Duration) -> Result<(), IntegratedError> {
        let mut preflight = self.estimator.clone();
        preflight
            .advance_to(at)
            .map_err(IntegratedError::Configuration)?;
        // Reserve enough history budget for the longest recovery lookahead.
        preflight
            .advance_to(
                at.checked_add(Duration::from_millis(6100))
                    .ok_or_else(|| IntegratedError::Configuration("time overflow".into()))?,
            )
            .map_err(IntegratedError::Configuration)?;
        while self.runtime.now() < at {
            let next = self.runtime.now() + TICK;
            self.estimator
                .advance_to(next)
                .map_err(IntegratedError::Configuration)?;
            self.runtime.advance_to(next)?;
            self.refresh_claims()?;
            self.monitor()?;
        }
        Ok(())
    }
    fn monitor(&mut self) -> Result<(), IntegratedError> {
        if self.policy != RefinementPolicy::B6 {
            for (id, _) in self.effects.values() {
                let instance = &self.runtime.instances()[id];
                let def = &self.runtime.definitions()[&instance.action_id];
                if instance.state.is_terminal()
                    && def.run_contract.as_ref().is_some_and(|root| {
                        self.runtime.assurance().evaluation().assurance_by_node[root].status()
                            != AssuranceStatus::Valid
                    })
                {
                    self.persistent_pending.insert(id.clone());
                }
            }
            return Ok(());
        }
        let effects: Vec<_> = self.effects.values().cloned().collect();
        for (id, action) in effects {
            let check = self.estimator.check_continuation(action);
            let unsupported = check.decision != Decision::Supported;
            self.audit.push(ModelAudit {
                instance: id.clone(),
                recorded_at: self.runtime.now(),
                phase: "persistent-monitor",
                check,
            });
            let recovery_unsupported =
                match self.choose_recovery(&id, None, "recovery-monitor", false) {
                    Ok(_) => false,
                    Err(IntegratedError::Blocked(_)) => true,
                    Err(e) => return Err(e),
                };
            if (unsupported || recovery_unsupported)
                && self.runtime.instances()[&id].state.is_terminal()
            {
                self.persistent_pending.insert(id.clone());
            }
            if (unsupported || recovery_unsupported)
                && matches!(
                    self.runtime.instances()[&id].state,
                    ActionState::Executing | ActionState::Committed
                )
            {
                self.runtime.request_stop(&id, AssuranceStatus::Unknown)?;
            }
        }
        Ok(())
    }
    /// Configured permission to mitigate a persistent effect survives task
    /// completion. Physical success must still be independently observed. This
    /// creates no synthetic runtime acknowledgement or completed outcome.
    pub fn recover_persistent(&mut self, id: &str) -> Result<PhysicalCommand, IntegratedError> {
        self.binding(id)?;
        if !self.runtime.instances()[id].state.is_terminal()
            || !self.effects.values().any(|(owner, _)| owner == id)
        {
            return Err(ActionError::IllegalTransition.into());
        }
        let recovery = self.choose_recovery(id, None, "persistent-recovery-selection", true)?;
        let command = self.admit(id, recovery, true)?;
        self.estimator
            .note_command(command.clone())
            .map_err(IntegratedError::Configuration)?;
        self.effects
            .insert(recovery.device(), (id.into(), recovery));
        self.persistent_pending.remove(id);
        self.refresh_claims()?;
        Ok(PhysicalCommand {
            runtime_command: None,
            model_command: command,
        })
    }
}
