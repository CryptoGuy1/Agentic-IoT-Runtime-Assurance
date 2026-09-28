#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/scenario.rs"]
mod scenario;
use dcra::evidence::{AssuranceStatus as Status, AssuranceValue as Value};
use dcra::graph::{AssuranceGraph, Node, NodeKind};
use dcra::runtime::{AssuranceRuntime, AuditEvent, EvaluationMode, EvaluationStats};
use scenario::{Operation, Scenario, atom, rule};
use std::time::Duration;

fn c_graph() -> AssuranceGraph {
    AssuranceGraph::new(
        vec![
            Node::new("s1", NodeKind::Evidence),
            Node::new("s2", NodeKind::Evidence),
            Node::new("thermal", NodeKind::Evidence),
            Node::new("occupancy", NodeKind::Evidence),
            Node::new("hazard", NodeKind::Derived),
            Node::new("fan", NodeKind::ActionStart),
            Node::new("alarm", NodeKind::ActionStart),
            Node::new("door", NodeKind::ActionStart),
        ],
        vec![
            rule("quorum", &["s1", "s2", "thermal"], 2, "hazard"),
            rule("fan", &["hazard"], 1, "fan"),
            rule("alarm", &["hazard"], 1, "alarm"),
            rule("door", &["occupancy"], 1, "door"),
        ],
    )
    .unwrap()
}

#[test]
fn c1_through_c4_preserve_support_log_substitution_and_isolate_unrelated_roots() {
    for mode in [EvaluationMode::Full, EvaluationMode::Incremental] {
        let mut runtime = AssuranceRuntime::with_mode(c_graph(), mode);
        for id in ["s1", "s2", "thermal", "occupancy"] {
            runtime.update(atom(id, Status::Valid, 10, 0, 2)).unwrap();
        }
        let at = runtime.audit().len();
        runtime
            .update(atom("s2", Status::Unknown, 11, 0, 2))
            .unwrap();
        assert_eq!(
            runtime.evaluation().assurance_by_node["hazard"],
            Value::Valid {
                horizon: Duration::from_secs(2)
            }
        );
        assert!(runtime.audit()[at..].iter().any(|entry| matches!(&entry.event,AuditEvent::WitnessChanged {node_id,previous:Some(old),current:Some(new)}
            if node_id=="fan" && old.evidence_ids.contains("s2") && new.evidence_ids.contains("thermal"))));
        // C4: exactly the occupancy leaf and door root are evaluated locally.
        let at = runtime.audit().len();
        runtime
            .update(atom("occupancy", Status::Unknown, 11, 0, 2))
            .unwrap();
        if mode == EvaluationMode::Incremental {
            assert_eq!(
                runtime.operation_stats(),
                EvaluationStats {
                    nodes_evaluated: 2,
                    justifications_evaluated: 1
                }
            );
        }
        assert_eq!(
            runtime.evaluation().assurance_by_node["door"],
            Value::Unknown
        );
        for id in ["fan", "alarm"] {
            assert!(matches!(
                runtime.evaluation().assurance_by_node[id],
                Value::Valid { .. }
            ));
            assert!(!runtime.audit()[at..].iter().any(|e| matches!(&e.event,AuditEvent::AssuranceChanged {node_id,..}|AuditEvent::WitnessChanged {node_id,..} if node_id==id)));
        }
        // C2: surviving evidence expires, retaining its observation version.
        runtime.advance_to(Duration::from_secs(3)).unwrap();
        assert_eq!(runtime.evidence()["s1"].version, 10);
        assert_eq!(
            runtime.evaluation().assurance_by_node["fan"],
            Value::Unknown
        );
        assert!(runtime.audit().iter().any(|e| e.at==Duration::from_secs(2) && matches!(&e.event,AuditEvent::EvidenceExpired {evidence_id,version:10} if evidence_id=="s1")));
    }
}

#[test]
fn version_only_refresh_stops_at_leaf_but_horizon_change_propagates() {
    let mut runtime = AssuranceRuntime::new(c_graph());
    runtime
        .update(atom("occupancy", Status::Valid, 1, 0, 2))
        .unwrap();
    runtime
        .update(atom("occupancy", Status::Valid, 2, 0, 2))
        .unwrap();
    assert_eq!(
        runtime.operation_stats(),
        EvaluationStats {
            nodes_evaluated: 1,
            justifications_evaluated: 0
        }
    );
    runtime
        .update(atom("occupancy", Status::Valid, 3, 0, 4))
        .unwrap();
    assert_eq!(
        runtime.operation_stats(),
        EvaluationStats {
            nodes_evaluated: 2,
            justifications_evaluated: 1
        }
    );
    assert_eq!(
        runtime.evaluation().assurance_by_node["door"],
        Value::Valid {
            horizon: Duration::from_secs(4)
        }
    );
    let stats = runtime.operation_stats();
    assert!(
        runtime
            .update(atom("occupancy", Status::Valid, 3, 0, 4))
            .is_err()
    );
    assert_eq!(runtime.operation_stats(), stats);
    runtime.advance_to(Duration::from_secs(1)).unwrap();
    assert_eq!(runtime.operation_stats(), EvaluationStats::default());
    assert_eq!(runtime.last_evaluation_stats(), EvaluationStats::default());
}

#[test]
fn alternative_rule_tie_changes_witness_while_status_and_horizon_stay_valid() {
    let nodes = vec![
        Node::new("a", NodeKind::Evidence),
        Node::new("b", NodeKind::Evidence),
        Node::new("left", NodeKind::Derived),
        Node::new("right", NodeKind::Derived),
        Node::new("out", NodeKind::Derived),
    ];
    let rules = vec![
        rule("left", &["a"], 1, "left"),
        rule("right", &["b"], 1, "right"),
        rule("z", &["right"], 1, "out"),
        rule("a", &["left"], 1, "out"),
    ];
    scenario::run(&Scenario {
        seed: 9001,
        nodes,
        rules,
        operations: vec![
            Operation::Update(atom("b", Status::Valid, 1, 0, 5)),
            Operation::Update(atom("a", Status::Valid, 1, 0, 5)),
            Operation::Update(atom("a", Status::Unknown, 2, 0, 5)),
        ],
    });
}

#[test]
fn simultaneous_expiry_and_multiple_batches_have_correct_work_counts() {
    let mut runtime = AssuranceRuntime::new(c_graph());
    runtime.update(atom("s1", Status::Valid, 1, 0, 2)).unwrap();
    runtime.update(atom("s2", Status::Valid, 1, 0, 2)).unwrap();
    runtime
        .update(atom("occupancy", Status::Valid, 1, 0, 4))
        .unwrap();
    runtime.advance_to(Duration::from_secs(5)).unwrap();
    assert_eq!(
        runtime.last_evaluation_stats(),
        EvaluationStats {
            nodes_evaluated: 2,
            justifications_evaluated: 1
        }
    );
    assert_eq!(
        runtime.operation_stats(),
        EvaluationStats {
            nodes_evaluated: 7,
            justifications_evaluated: 4
        }
    );
}

#[test]
fn generated_small_graphs_agree_with_full_and_exhaustive_oracles() {
    for seed in 0..128 {
        scenario::run(&scenario::generate(seed, 2 + (seed as usize % 9), 32));
    }
}
#[test]
fn generated_large_graphs_agree_after_every_operation_and_expiry_batch() {
    for seed in 0..128 {
        scenario::run(&scenario::generate(
            10_000 + seed,
            16 + (seed as usize % 85),
            128,
        ));
    }
}
#[test]
fn trace_round_trip_preserves_metadata_and_nanoseconds() {
    let mut s = scenario::generate(42, 8, 12);
    let mut a = atom("n000", Status::Unknown, 500, 0, 3);
    a.source_id = "source with\nspaces".into();
    a.predicate = String::new();
    a.payload_hash = Some("hash\tvalue".into());
    a.expires_at = Duration::new(3, 123);
    s.operations.push(Operation::Update(a));
    assert_eq!(Scenario::decode(&s.encode()).unwrap(), s);
    scenario::run(&Scenario::decode(&s.encode()).unwrap());
    assert!(Scenario::decode("DCRA_TRACE_V2 1").is_err());
}
#[test]
#[ignore = "set DCRA_REPLAY to a saved .trace file"]
fn replay_saved_trace() {
    let path = std::env::var("DCRA_REPLAY").expect("set DCRA_REPLAY to the trace path");
    let scenario = Scenario::decode(&std::fs::read_to_string(path).unwrap()).unwrap();
    scenario::run(&scenario);
}

#[test]
fn saved_event_boundary_trace_replays() {
    let scenario = Scenario::decode(include_str!("fixtures/event-boundaries.trace")).unwrap();
    scenario::run(&scenario);
}

#[test]
fn shared_leaves_and_alternative_size_ties_match_in_both_modes() {
    let graph = AssuranceGraph::new(
        vec![
            Node::new("a", NodeKind::Evidence),
            Node::new("b", NodeKind::Evidence),
            Node::new("left", NodeKind::Derived),
            Node::new("right", NodeKind::Derived),
            Node::new("out", NodeKind::Derived),
        ],
        vec![
            rule("left", &["a"], 1, "left"),
            rule("right", &["a"], 1, "right"),
            rule("z-small", &["left", "right"], 2, "out"),
            rule("a-small", &["b"], 1, "out"),
            rule("0-large", &["a", "b"], 2, "out"),
        ],
    )
    .unwrap();
    let mut full = AssuranceRuntime::with_mode(graph.clone(), EvaluationMode::Full);
    let mut incremental = AssuranceRuntime::new(graph);
    for runtime in [&mut full, &mut incremental] {
        runtime.update(atom("a", Status::Valid, 1, 0, 8)).unwrap();
        assert_eq!(
            runtime.evaluation().preferred_witness_by_node["out"]
                .evidence_ids
                .len(),
            1
        );
        runtime.update(atom("b", Status::Valid, 1, 0, 8)).unwrap();
        assert_eq!(
            runtime.evaluation().preferred_witness_by_node["out"]
                .justification_id
                .as_deref(),
            Some("a-small")
        );
        runtime.update(atom("b", Status::Unknown, 2, 0, 8)).unwrap();
        assert_eq!(
            runtime.evaluation().preferred_witness_by_node["out"]
                .justification_id
                .as_deref(),
            Some("z-small")
        );
        oracle::verify(
            runtime.graph(),
            runtime.evidence(),
            runtime.now(),
            runtime.evaluation(),
        );
    }
    assert_eq!(full.evaluation(), incremental.evaluation());
    assert_eq!(full.audit(), incremental.audit());
}
