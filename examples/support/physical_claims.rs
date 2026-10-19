use dcra::actions::*;
use dcra::evidence::{AssuranceStatus as Status, EvidenceAtom};
use dcra::graph::{AssuranceGraph, Justification, Node, NodeKind};
use dcra::model_v1::*;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
pub fn ms(t: u64) -> Duration {
    Duration::from_millis(t)
}
pub fn atom(id: &str, status: Status, version: u64, expires: Duration) -> EvidenceAtom {
    EvidenceAtom {
        evidence_id: id.into(),
        evidence_type: "direct".into(),
        predicate: id.into(),
        source_id: id.into(),
        version,
        observed_at: Duration::ZERO,
        expires_at: expires,
        status,
        payload_hash: None,
    }
}
pub fn configuration() -> ClaimConfiguration {
    ClaimConfiguration {
        revision: "claim-fixtures-v1".into(),
        replacements: vec![PhysicalClaimBinding {
            direct_evidence: "pressure.direct".into(),
            model_evidence: "pressure.model".into(),
            predicate: PhysicalPredicate::PressureWithinLimit,
        }],
        protected_evidence: BTreeSet::from(["permit".into()]),
    }
}
pub fn graph_and_definition() -> (AssuranceGraph, Vec<ActionDefinition>) {
    let mut nodes: Vec<_> = ["permit", "pressure.direct", "ack", "effect"]
        .iter()
        .map(|id| Node::new(*id, NodeKind::Evidence))
        .collect();
    nodes.extend([
        Node::new("start", NodeKind::ActionStart),
        Node::new("run", NodeKind::ActionRun),
        Node::new("commit", NodeKind::ActionCommit),
        Node::new("outcome", NodeKind::ActionOutcome),
    ]);
    let rule = |id: &str, premises: &[&str]| Justification {
        justification_id: format!("{id}.rule"),
        premises: premises.iter().map(|p| (*p).into()).collect(),
        threshold: premises.len(),
        conclusion: id.into(),
    };
    let graph = AssuranceGraph::new(
        nodes,
        vec![
            rule("start", &["permit", "pressure.direct"]),
            rule("run", &["permit", "pressure.direct"]),
            rule("commit", &["permit", "pressure.direct"]),
            rule("outcome", &["ack", "effect"]),
        ],
    )
    .unwrap();
    let def = ActionDefinition {
        action_id: "alarm".into(),
        start_contract: "start".into(),
        run_contract: Some("run".into()),
        commit_contract: Some("commit".into()),
        outcome_contract: "outcome".into(),
        min_duration: ms(200),
        max_duration: ms(10000),
        interruptibility: Interruptibility::Irreversible,
        recovery_type: RecoveryType::Irreversible,
        resources_read: BTreeSet::new(),
        resources_written: BTreeSet::new(),
        max_start_lease: ms(1000),
        controllers: ControllerCapabilities {
            safe_abort: true,
            forward_mitigation: true,
            ..ControllerCapabilities::default()
        },
        outcomes: OutcomeBindings {
            acknowledgement: "ack".into(),
            observed: BTreeMap::from([("on".into(), "effect".into())]),
            max_age: ms(1000),
        },
    };
    (graph, vec![def])
}
pub fn reading(id: &str, version: u64, value: f64, at: Duration) -> Observation {
    Observation {
        id: id.into(),
        source: "pressure-sensor".into(),
        variable: Variable::Pressure,
        value: Reading::Number(value),
        error: Some(0.03),
        generated_at: at,
        received_at: at,
        max_age: ms(500),
        version,
        quality: Quality::Trusted,
    }
}
pub fn estimator(value: f64) -> Estimator {
    let p = |v| Interval::point(v).unwrap();
    let mut state = StateBox::unknown();
    state.hazard = p(0.0);
    state.fan = p(0.0);
    state.door = p(0.0);
    state.occupied = BoolSet::False;
    state.alarm = BoolSet::False;
    state.fan_failed = BoolSet::False;
    state.door_jammed = BoolSet::False;
    state.response_required = BoolSet::False;
    let mut estimator = Estimator::new(
        state,
        Inputs::default(),
        Assumptions {
            source: p(0.0),
            faults_may_start: false,
            occupancy_stable_until: ms(20000),
            ..Assumptions::default()
        },
    )
    .unwrap();
    estimator
        .observe(reading("pressure-1", 1, value, Duration::ZERO))
        .unwrap();
    estimator
}
pub fn bindings() -> BTreeMap<String, ModelBinding> {
    BTreeMap::from([(
        "alarm".into(),
        ModelBinding {
            action: Action::AlarmOn,
            recovery: Action::AlarmOn,
            max_delay: ms(100),
            may_drop: false,
        },
    )])
}
pub fn plan() -> Plan {
    Plan {
        plan_id: "fixture".into(),
        version: 1,
        actions: BTreeMap::from([("a".into(), "alarm".into())]),
        precedence: BTreeSet::new(),
        created_at: Duration::ZERO,
    }
}
pub fn direct(value: f64) -> ActionRuntime {
    let (graph, definitions) = graph_and_definition();
    let mut runtime = ActionRuntime::new(graph, definitions).unwrap();
    runtime
        .update(atom("permit", Status::Valid, 1, ms(20000)))
        .unwrap();
    // This comparator uses a direct point-threshold predicate and a 500 ms TTL.
    // It has no interval-error or model-propagation capability.
    runtime
        .update(atom(
            "pressure.direct",
            if value.abs() <= 0.80 {
                Status::Valid
            } else {
                Status::Invalid
            },
            1,
            ms(500),
        ))
        .unwrap();
    runtime.register_plan(plan()).unwrap();
    runtime
}
pub fn model(value: f64) -> ModelRuntime {
    let (graph, definitions) = graph_and_definition();
    let config = configuration();
    let graph = substitute_physical_claims(&graph, &definitions, &config).unwrap();
    let runtime = ActionRuntime::new(graph, definitions).unwrap();
    let mut runtime =
        ModelRuntime::with_claims(runtime, estimator(value), bindings(), config).unwrap();
    runtime
        .update_permission(atom("permit", Status::Valid, 1, ms(20000)))
        .unwrap();
    // Preserve the raw leaf for trace/comparison, but it is no longer a conjunct.
    runtime
        .update_permission(atom(
            "pressure.direct",
            if value.abs() <= 0.80 {
                Status::Valid
            } else {
                Status::Invalid
            },
            1,
            ms(500),
        ))
        .unwrap();
    runtime.register_plan(plan()).unwrap();
    runtime
}
