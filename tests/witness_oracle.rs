#[path = "support/oracle.rs"]
mod oracle;
use dcra::evaluator::evaluate_full;
use dcra::evidence::{AssuranceStatus as Status, EvidenceAtom};
use dcra::graph::{AssuranceGraph, Justification, Node, NodeKind};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

fn atom(id: &str, expiry: u64) -> EvidenceAtom {
    EvidenceAtom {
        evidence_id: id.into(),
        evidence_type: "sensor".into(),
        predicate: "high".into(),
        source_id: id.into(),
        version: 1,
        observed_at: Duration::ZERO,
        expires_at: Duration::from_secs(expiry),
        status: Status::Valid,
        payload_hash: None,
    }
}
fn threshold(count: usize, k: usize) -> AssuranceGraph {
    let ids: Vec<_> = (0..count).map(|i| format!("s{i}")).collect();
    let mut nodes: Vec<_> = ids
        .iter()
        .map(|id| Node::new(id, NodeKind::Evidence))
        .collect();
    nodes.push(Node::new("out", NodeKind::Derived));
    AssuranceGraph::new(
        nodes,
        vec![Justification {
            justification_id: "rule".into(),
            premises: ids,
            threshold: k,
            conclusion: "out".into(),
        }],
    )
    .unwrap()
}
#[test]
fn oracle_matches_hand_worked_support_sets_before_checking_runtime() {
    let graph = threshold(3, 2);
    let evidence = BTreeMap::from([
        ("s0".into(), atom("s0", 5)),
        ("s1".into(), atom("s1", 8)),
        ("s2".into(), atom("s2", 12)),
    ]);
    let sets = oracle::enumerate(&graph, &evidence, Duration::ZERO);
    let support = |ids: &[&str]| {
        ids.iter()
            .map(|id| (*id).to_owned())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        sets["out"],
        BTreeMap::from([
            (support(&["s0", "s1"]), Duration::from_secs(5)),
            (support(&["s0", "s2"]), Duration::from_secs(5)),
            (support(&["s1", "s2"]), Duration::from_secs(8)),
            (support(&["s0", "s1", "s2"]), Duration::from_secs(5)),
        ])
    );
    for now in [0, 5, 8, 12] {
        let now = Duration::from_secs(now);
        oracle::verify(
            &graph,
            &evidence,
            now,
            &evaluate_full(&graph, &evidence, now),
        );
    }
}
#[test]
fn fifteen_node_boundary_is_exhaustive() {
    let graph = threshold(14, 7);
    let evidence = (0..14)
        .map(|i| {
            let id = format!("s{i}");
            (id.clone(), atom(&id, i + 1))
        })
        .collect();
    let result = evaluate_full(&graph, &evidence, Duration::ZERO);
    assert_eq!(
        result.assurance_by_node["out"].horizon(),
        Some(Duration::from_secs(8))
    );
    oracle::verify(&graph, &evidence, Duration::ZERO, &result);
}
#[test]
fn checker_rejects_a_nonmaximal_but_valid_runtime_witness() {
    let graph = threshold(3, 2);
    let evidence = BTreeMap::from([
        ("s0".into(), atom("s0", 5)),
        ("s1".into(), atom("s1", 8)),
        ("s2".into(), atom("s2", 12)),
    ]);
    let mut result = evaluate_full(&graph, &evidence, Duration::ZERO);
    let witness = result.preferred_witness_by_node.get_mut("out").unwrap();
    witness.evidence_ids = BTreeSet::from(["s0".into(), "s1".into()]);
    witness.horizon = Duration::from_secs(5);
    assert!(
        std::panic::catch_unwind(|| oracle::verify(&graph, &evidence, Duration::ZERO, &result))
            .is_err()
    );
}
