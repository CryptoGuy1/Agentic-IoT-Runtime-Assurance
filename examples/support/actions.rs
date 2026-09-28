use dcra::actions::*;
use dcra::evidence::{AssuranceStatus as Status, EvidenceAtom};
use dcra::graph::{AssuranceGraph, Justification, Node, NodeKind};
use dcra::runtime::EvaluationMode;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
pub fn secs(t: u64) -> Duration {
    Duration::from_secs(t)
}
pub fn observation(id: &str, status: Status, version: u64, expires: u64) -> EvidenceAtom {
    EvidenceAtom {
        evidence_id: id.into(),
        evidence_type: "sensor".into(),
        source_id: id.into(),
        predicate: "condition".into(),
        version,
        observed_at: Duration::ZERO,
        expires_at: secs(expires),
        status,
        payload_hash: None,
    }
}
pub fn configuration(
    class: Interruptibility,
    recovery: RecoveryType,
) -> (AssuranceGraph, Vec<ActionDefinition>) {
    let evidence = [
        "start", "run_a", "run_b", "commit", "other", "a.ack", "a.done", "b.ack", "b.done",
    ];
    let mut nodes: Vec<_> = evidence
        .iter()
        .map(|id| Node::new(*id, NodeKind::Evidence))
        .collect();
    nodes.extend([
        Node::new("a.start", NodeKind::ActionStart),
        Node::new("a.run", NodeKind::ActionRun),
        Node::new("a.commit", NodeKind::ActionCommit),
        Node::new("a.outcome", NodeKind::ActionOutcome),
        Node::new("b.start", NodeKind::ActionStart),
        Node::new("b.outcome", NodeKind::ActionOutcome),
    ]);
    let r = |id: &str, p: &[&str], threshold| Justification {
        justification_id: format!("rule-{id}"),
        premises: p.iter().map(|s| (*s).into()).collect(),
        threshold,
        conclusion: id.into(),
    };
    let graph = AssuranceGraph::new(
        nodes,
        vec![
            r("a.start", &["start"], 1),
            r("a.run", &["run_a", "run_b"], 1),
            r("a.commit", &["commit"], 1),
            r("a.outcome", &["a.ack", "a.done"], 2),
            r("b.start", &["a.outcome"], 1),
            r("b.outcome", &["b.ack", "b.done"], 2),
        ],
    )
    .unwrap();
    let definition = |id: &str, prefix: &str| ActionDefinition {
        action_id: id.into(),
        start_contract: format!("{prefix}.start"),
        run_contract: None,
        commit_contract: None,
        outcome_contract: format!("{prefix}.outcome"),
        min_duration: secs(2),
        max_duration: secs(10),
        interruptibility: Interruptibility::Preemptible,
        recovery_type: RecoveryType::Reversible,
        resources_read: BTreeSet::new(),
        resources_written: BTreeSet::new(),
        max_start_lease: secs(3),
        controllers: ControllerCapabilities {
            safe_abort: true,
            certified_complete: true,
            forward_mitigation: true,
            compensate: true,
        },
        outcomes: OutcomeBindings {
            acknowledgement: format!("{prefix}.ack"),
            observed: BTreeMap::from([("done".into(), format!("{prefix}.done"))]),
            max_age: secs(20),
        },
    };
    let mut first = definition("fan", "a");
    first.run_contract = Some("a.run".into());
    first.interruptibility = class;
    first.recovery_type = recovery;
    if class == Interruptibility::Irreversible {
        first.commit_contract = Some("a.commit".into());
    }
    (graph, vec![first, definition("follow", "b")])
}
pub fn runtime(
    mode: EvaluationMode,
    class: Interruptibility,
    recovery: RecoveryType,
) -> ActionRuntime {
    let (graph, definitions) = configuration(class, recovery);
    let mut runtime = ActionRuntime::with_mode(graph, definitions, mode).unwrap();
    for id in ["start", "run_a", "commit", "other"] {
        runtime
            .update(observation(id, Status::Valid, 1, 100))
            .unwrap();
    }
    runtime
        .register_plan(Plan {
            plan_id: "plan".into(),
            version: 7,
            actions: BTreeMap::from([("a".into(), "fan".into()), ("b".into(), "follow".into())]),
            precedence: BTreeSet::from([("a".into(), "b".into())]),
            created_at: Duration::ZERO,
        })
        .unwrap();
    runtime
}
pub fn start(runtime: &mut ActionRuntime) -> String {
    let token = runtime.request_lease("a", ActionPhase::Start).unwrap();
    runtime.dispatch("a", ActionPhase::Start, &token).unwrap()
}
pub fn report(
    command_id: &str,
    sequence: u64,
    now: Duration,
    ack: Option<bool>,
    completed: bool,
    done: Option<Status>,
) -> ActuatorResult {
    ActuatorResult {
        command_id: command_id.into(),
        sequence,
        acknowledged: ack,
        completed,
        observed_at: now,
        observed_state: done.map_or_else(BTreeMap::new, |s| BTreeMap::from([("done".into(), s)])),
    }
}
