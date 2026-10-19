//! Versioned physical claims. These never stand for permission or actuator success.
use super::{estimate::predict, *};
use crate::actions::ActionDefinition;
use crate::evidence::{AssuranceStatus, EvidenceAtom};
use crate::graph::{AssuranceGraph, Node, NodeKind};
use std::collections::BTreeSet;
use std::time::Duration;

pub const CLAIM_SCHEMA: &str = "physical-claims-v1";
/// Claims describe all represented states through their short validity window.
/// Candidate action consequences are checked separately at release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalPredicate {
    PressureWithinLimit,
    OccupancyClear,
    FanStopped,
}
impl PhysicalPredicate {
    fn status(self, state: &StateBox) -> AssuranceStatus {
        use AssuranceStatus::*;
        match self {
            Self::PressureWithinLimit
                if state.pressure.lo() >= -0.80 && state.pressure.hi() <= 0.80 =>
            {
                Valid
            }
            Self::PressureWithinLimit
                if state.pressure.lo() > 0.80 || state.pressure.hi() < -0.80 =>
            {
                Invalid
            }
            Self::OccupancyClear if state.occupied == BoolSet::False => Valid,
            Self::OccupancyClear if state.occupied == BoolSet::True => Invalid,
            Self::FanStopped if state.fan.hi() <= 0.05 => Valid,
            Self::FanStopped if state.fan.lo() > 0.05 => Invalid,
            _ => Unknown,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalClaimBinding {
    pub direct_evidence: String,
    pub model_evidence: String,
    pub predicate: PhysicalPredicate,
}
#[derive(Clone, Debug)]
pub struct ClaimConfiguration {
    /// Identifies the reviewed graph/physical mapping and model configuration.
    /// Immutable for one runtime; a changed configuration needs a new runtime.
    pub revision: String,
    pub replacements: Vec<PhysicalClaimBinding>,
    pub protected_evidence: BTreeSet<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ObservationReference {
    pub id: String,
    pub source: String,
    pub variable: Variable,
    pub version: u64,
    pub generated_at: Duration,
    pub received_at: Duration,
    pub max_age: Duration,
    pub quality: Quality,
    pub reading: Reading,
    pub error: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalCertificate {
    pub schema: &'static str,
    pub model_version: &'static str,
    pub configuration_revision: String,
    pub evidence_id: String,
    pub predicate: PhysicalPredicate,
    pub version: u64,
    pub state_revision: u64,
    pub checked_at: Duration,
    /// Exclusive for authority; the trajectory also checks this boundary.
    pub valid_until: Duration,
    pub status: AssuranceStatus,
    pub observations: Vec<ObservationReference>,
    pub command_ids: Vec<u64>,
    pub assumptions: Assumptions,
    pub state: StateBox,
    pub initial_state: StateBox,
    pub initial_inputs: Inputs,
    pub affected_contracts: Vec<String>,
    pub reason: String,
}
impl PhysicalCertificate {
    pub(crate) fn atom(&self) -> EvidenceAtom {
        EvidenceAtom {
            evidence_id: self.evidence_id.clone(),
            evidence_type: CLAIM_SCHEMA.into(),
            predicate: format!("{:?}", self.predicate),
            source_id: "model-claim-producer".into(),
            version: self.version,
            observed_at: self.checked_at,
            expires_at: self.valid_until,
            status: self.status,
            // A trace reference, not a cryptographic integrity claim.
            payload_hash: Some(format!(
                "{}:{}:{}:{}",
                CLAIM_SCHEMA, self.configuration_revision, self.state_revision, self.version
            )),
        }
    }
}
pub(crate) fn ancestors(graph: &AssuranceGraph, root: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(id) = stack.pop() {
        if !result.insert(id.clone()) {
            continue;
        }
        if let Some(rules) = graph.justifications_by_conclusion().get(&id) {
            for rule in rules {
                stack.extend(graph.justifications()[rule].premises.iter().cloned());
            }
        }
    }
    result
}
/// Explicit graph migration: replace only reviewed physical leaves. Rule
/// thresholds and all other premises remain byte-for-byte equivalent. Historical
/// graphs are returned unchanged to their callers and remain valid comparators.
pub fn substitute_physical_claims(
    graph: &AssuranceGraph,
    definitions: &[ActionDefinition],
    configuration: &ClaimConfiguration,
) -> Result<AssuranceGraph, String> {
    validate_mapping(graph, definitions, configuration, false)?;
    let mut nodes: Vec<_> = graph
        .nodes()
        .iter()
        .map(|(id, kind)| Node::new(id, *kind))
        .collect();
    for b in &configuration.replacements {
        nodes.push(Node::new(&b.model_evidence, NodeKind::Evidence));
    }
    let mut rules: Vec<_> = graph.justifications().values().cloned().collect();
    for rule in &mut rules {
        for premise in &mut rule.premises {
            if let Some(b) = configuration
                .replacements
                .iter()
                .find(|b| b.direct_evidence == *premise)
            {
                *premise = b.model_evidence.clone();
            }
        }
    }
    AssuranceGraph::new(nodes, rules).map_err(|e| e.to_string())
}
pub(crate) fn validate_mapping(
    graph: &AssuranceGraph,
    definitions: &[ActionDefinition],
    config: &ClaimConfiguration,
    migrated: bool,
) -> Result<(), String> {
    if config.revision.trim().is_empty()
        || config.replacements.is_empty()
        || definitions.is_empty()
        || config.protected_evidence.is_empty()
        || config
            .protected_evidence
            .iter()
            .any(|id| graph.nodes().get(id) != Some(&NodeKind::Evidence))
    {
        return Err("claims require a configuration revision, explicit physical mappings and protected permission evidence".into());
    }
    let outcomes: BTreeSet<_> = definitions
        .iter()
        .flat_map(|d| ancestors(graph, &d.outcome_contract))
        .collect();
    let phase_nodes: BTreeSet<_> = definitions
        .iter()
        .flat_map(|d| {
            std::iter::once(&d.start_contract)
                .chain(d.run_contract.iter())
                .chain(d.commit_contract.iter())
                .flat_map(|r| ancestors(graph, r))
        })
        .collect();
    let mut direct = BTreeSet::new();
    let mut model = BTreeSet::new();
    for b in &config.replacements {
        let used = if migrated {
            &b.model_evidence
        } else {
            &b.direct_evidence
        };
        if b.model_evidence.trim().is_empty()
            || b.direct_evidence == b.model_evidence
            || !direct.insert(b.direct_evidence.clone())
            || !model.insert(b.model_evidence.clone())
            || graph.nodes().get(&b.direct_evidence) != Some(&NodeKind::Evidence)
            || config.protected_evidence.contains(&b.direct_evidence)
            || config.protected_evidence.contains(&b.model_evidence)
            || outcomes.contains(&b.direct_evidence)
            || outcomes.contains(&b.model_evidence)
            || !phase_nodes.contains(used)
            || if migrated {
                graph.nodes().get(&b.model_evidence) != Some(&NodeKind::Evidence)
                    || phase_nodes.contains(&b.direct_evidence)
            } else {
                graph.nodes().contains_key(&b.model_evidence)
            }
        {
            return Err("physical replacement must be unique, used in admission/run/commit, and separate from permission and outcome evidence".into());
        }
    }
    if !direct.is_disjoint(&model) {
        return Err("physical mapping IDs overlap".into());
    }
    Ok(())
}
pub(crate) fn produce(
    estimator: &Estimator,
    config: &ClaimConfiguration,
    binding: &PhysicalClaimBinding,
    version: u64,
    affected_contracts: Vec<String>,
) -> Result<PhysicalCertificate, String> {
    let until = estimator
        .state()
        .at
        .checked_add(TICK * 2)
        .ok_or("claim time overflow")?;
    let mut validation = estimator.clone();
    validation.advance_to(until)?;
    let mut state = estimator.state().clone();
    let mut status = AssuranceStatus::Valid;
    loop {
        match binding.predicate.status(&state) {
            AssuranceStatus::Invalid => status = AssuranceStatus::Invalid,
            AssuranceStatus::Unknown if status != AssuranceStatus::Invalid => {
                status = AssuranceStatus::Unknown
            }
            _ => {}
        }
        if state.at == until {
            break;
        }
        state = predict(
            &state,
            estimator.assumptions(),
            estimator.initial_inputs(),
            estimator.commands(),
        );
    }
    if estimator.inconsistent() {
        status = AssuranceStatus::Unknown;
    }
    Ok(PhysicalCertificate {
        schema: CLAIM_SCHEMA,
        model_version: MODEL_VERSION,
        configuration_revision: config.revision.clone(),
        evidence_id: binding.model_evidence.clone(),
        predicate: binding.predicate,
        version,
        state_revision: estimator.revision(),
        checked_at: estimator.state().at,
        valid_until: until,
        status,
        observations: estimator
            .accepted_observations()
            .map(|o| ObservationReference {
                id: o.id.clone(),
                source: o.source.clone(),
                variable: o.variable,
                version: o.version,
                generated_at: o.generated_at,
                received_at: o.received_at,
                max_age: o.max_age,
                quality: o.quality,
                reading: o.value,
                error: o.error,
            })
            .collect(),
        command_ids: estimator.commands().iter().map(|c| c.id).collect(),
        assumptions: estimator.assumptions().clone(),
        state: estimator.state().clone(),
        initial_state: estimator.initial_state().clone(),
        initial_inputs: estimator.initial_inputs(),
        affected_contracts,
        reason: if estimator.inconsistent() {
            "Inconsistent model knowledge cannot support a physical claim.".into()
        } else {
            format!(
                "{status:?}: {:?} over all represented grid states from {:?} through {:?}; conditional on recorded assumptions.",
                binding.predicate,
                estimator.state().at,
                until
            )
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn certificate(
        predicate: PhysicalPredicate,
        pressure: f64,
        fan: f64,
        stable_ms: u64,
    ) -> PhysicalCertificate {
        let p = |v| Interval::point(v).unwrap();
        let mut state = StateBox::unknown();
        state.hazard = p(0.0);
        state.pressure = p(pressure);
        state.fan = p(fan);
        state.door = p(0.0);
        state.occupied = BoolSet::False;
        state.alarm = BoolSet::False;
        state.fan_failed = BoolSet::False;
        state.door_jammed = BoolSet::False;
        state.response_required = BoolSet::False;
        let estimator = Estimator::new(
            state,
            Inputs::default(),
            Assumptions {
                faults_may_start: false,
                occupancy_stable_until: Duration::from_millis(stable_ms),
                ..Assumptions::default()
            },
        )
        .unwrap();
        let binding = PhysicalClaimBinding {
            direct_evidence: "direct".into(),
            model_evidence: "model".into(),
            predicate,
        };
        let config = ClaimConfiguration {
            revision: "unit".into(),
            replacements: vec![binding.clone()],
            protected_evidence: BTreeSet::from(["permission".into()]),
        };
        produce(&estimator, &config, &binding, 1, vec![]).unwrap()
    }
    #[test]
    fn pressure_boundary_is_inclusive_but_the_next_float_outside_is_not_supported() {
        assert_eq!(
            certificate(PhysicalPredicate::PressureWithinLimit, 0.80, 0.0, 1000).status,
            AssuranceStatus::Valid
        );
        assert_eq!(
            certificate(
                PhysicalPredicate::PressureWithinLimit,
                0.80_f64.next_up(),
                0.0,
                1000
            )
            .status,
            AssuranceStatus::Invalid
        );
    }
    #[test]
    fn clearance_requires_stability_through_the_entire_checked_window() {
        assert_eq!(
            certificate(PhysicalPredicate::OccupancyClear, 0.0, 0.0, 300).status,
            AssuranceStatus::Valid
        );
        assert_eq!(
            certificate(PhysicalPredicate::OccupancyClear, 0.0, 0.0, 200).status,
            AssuranceStatus::Unknown
        );
        assert_eq!(
            certificate(PhysicalPredicate::OccupancyClear, 0.0, 0.0, 0).status,
            AssuranceStatus::Unknown
        );
    }
    #[test]
    fn fan_stop_threshold_is_inclusive() {
        assert_eq!(
            certificate(PhysicalPredicate::FanStopped, 0.0, 0.05, 1000).status,
            AssuranceStatus::Valid
        );
        assert_eq!(
            certificate(PhysicalPredicate::FanStopped, 0.0, 0.05_f64.next_up(), 1000).status,
            AssuranceStatus::Invalid
        );
    }
}
