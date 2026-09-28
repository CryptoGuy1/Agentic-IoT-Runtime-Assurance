//! Exhaustive test-only oracle. Deliberately does not call either evaluator,
//! EvidenceAtom::value_at, or production witness selection.
use dcra::evaluator::Evaluation;
use dcra::evidence::{AssuranceStatus, EvidenceAtom};
use dcra::graph::{AssuranceGraph, NodeKind};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

type Supports = BTreeMap<String, BTreeMap<BTreeSet<String>, Duration>>;

pub fn enumerate(
    graph: &AssuranceGraph,
    evidence: &BTreeMap<String, EvidenceAtom>,
    now: Duration,
) -> Supports {
    assert!(
        graph.nodes().len() <= 15,
        "exhaustive oracle limited to 15 total nodes"
    );
    let atoms: Vec<_> = graph
        .nodes()
        .iter()
        .filter(|(_, kind)| **kind == NodeKind::Evidence)
        .filter_map(|(id, _)| evidence.get(id).filter(|a| a.evidence_id == *id))
        .filter(|a| {
            a.status == AssuranceStatus::Valid && a.observed_at <= now && now < a.expires_at
        })
        .collect();
    let mut supports: Supports = graph
        .nodes()
        .keys()
        .map(|id| (id.clone(), BTreeMap::new()))
        .collect();
    for mask in 1usize..(1usize << atoms.len()) {
        let selected: Vec<_> = atoms
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, atom)| *atom)
            .collect();
        let ids: BTreeSet<_> = selected.iter().map(|a| a.evidence_id.clone()).collect();
        let horizon = selected.iter().map(|a| a.expires_at).min().unwrap();
        let mut satisfied = BTreeMap::new();
        for id in graph.topological_order() {
            let yes = if graph.nodes()[id] == NodeKind::Evidence {
                ids.contains(id)
            } else {
                // Scan rules independently, rather than using production evaluation helpers.
                graph
                    .justifications()
                    .values()
                    .filter(|r| r.conclusion == *id)
                    .any(|r| {
                        r.premises
                            .iter()
                            .filter(|p| satisfied.get(*p) == Some(&true))
                            .count()
                            >= r.threshold
                    })
            };
            satisfied.insert(id.clone(), yes);
            if yes {
                supports.get_mut(id).unwrap().insert(ids.clone(), horizon);
            }
        }
    }
    supports
}

pub fn verify(
    graph: &AssuranceGraph,
    evidence: &BTreeMap<String, EvidenceAtom>,
    now: Duration,
    evaluation: &Evaluation,
) {
    let supports = enumerate(graph, evidence, now);
    for (id, sets) in supports {
        let best = sets.values().copied().max();
        assert_eq!(
            evaluation.assurance_by_node[&id].horizon(),
            best,
            "maximum horizon for {id}"
        );
        match evaluation.preferred_witness_by_node.get(&id) {
            Some(witness) => {
                assert_eq!(witness.node_id, id);
                assert_eq!(
                    Some(&witness.horizon),
                    sets.get(&witness.evidence_ids),
                    "unsupported witness for {id}"
                );
                assert_eq!(Some(witness.horizon), best, "non-maximal witness for {id}");
            }
            None => assert!(best.is_none(), "missing witness for {id}"),
        }
    }
}
