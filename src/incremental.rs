//! Internal localized evaluator. The runtime owns the cache; this scheduler owns
//! graph ranks. Keep evaluation logic independent of the full reference evaluator.
use crate::evaluator::{Evaluation, Witness};
use crate::evidence::{AssuranceValue as Value, EvidenceAtom};
use crate::graph::{AssuranceGraph, NodeKind};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

/// Evaluation work only: excludes expiry discovery, logging and initialization.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationStats {
    pub nodes_evaluated: usize,
    pub justifications_evaluated: usize,
}

#[derive(Debug)]
pub(crate) struct Change {
    pub id: String,
    pub before: Value,
    pub after: Value,
    pub old_witness: Option<Witness>,
    pub new_witness: Option<Witness>,
}

#[derive(Debug)]
pub(crate) struct IncrementalEvaluator {
    ranks: BTreeMap<String, usize>,
}

impl IncrementalEvaluator {
    pub fn new(graph: &AssuranceGraph) -> Self {
        Self {
            ranks: graph
                .topological_order()
                .iter()
                .enumerate()
                .map(|(rank, id)| (id.clone(), rank))
                .collect(),
        }
    }

    pub fn update(
        &self,
        graph: &AssuranceGraph,
        evidence: &BTreeMap<String, EvidenceAtom>,
        now: Duration,
        result: &mut Evaluation,
        dirty: &[String],
    ) -> (Vec<Change>, EvaluationStats) {
        let mut queue: BTreeSet<_> = dirty
            .iter()
            .map(|id| (self.ranks[id], id.clone()))
            .collect();
        let mut changes = Vec::new();
        let mut stats = EvaluationStats::default();
        while let Some((_, id)) = queue.pop_first() {
            stats.nodes_evaluated += 1;
            let before = result
                .assurance_by_node
                .get(&id)
                .copied()
                .unwrap_or(Value::Unknown);
            let old_witness = result.preferred_witness_by_node.get(&id).cloned();
            let (after, new_witness) = if graph.nodes()[&id] == NodeKind::Evidence {
                let value = evidence
                    .get(&id)
                    .filter(|a| a.evidence_id == id)
                    .map_or(Value::Unknown, |a| a.value_at(now));
                let witness = value.horizon().map(|horizon| Witness {
                    node_id: id.clone(),
                    evidence_ids: BTreeSet::from([id.clone()]),
                    horizon,
                    justification_id: None,
                    premise_ids: Vec::new(),
                });
                (value, witness)
            } else {
                let mut best: Option<Witness> = None;
                let mut unknown_rule = false;
                for rule_id in &graph.justifications_by_conclusion()[&id] {
                    stats.justifications_evaluated += 1;
                    let rule = &graph.justifications()[rule_id];
                    let mut available = Vec::new();
                    let mut unknown = 0;
                    for premise in &rule.premises {
                        match result.assurance_by_node[premise] {
                            Value::Valid { .. } => {
                                available.push(&result.preferred_witness_by_node[premise])
                            }
                            Value::Unknown => unknown += 1,
                            Value::Invalid => {}
                        }
                    }
                    let rule_value = if available.len() < rule.threshold {
                        if available.len() + unknown < rule.threshold {
                            Value::Invalid
                        } else {
                            unknown_rule = true;
                            Value::Unknown
                        }
                    } else {
                        available.sort_by_key(|w| {
                            (Reverse(w.horizon), w.evidence_ids.len(), w.node_id.as_str())
                        });
                        let chosen = &available[..rule.threshold];
                        let horizon = chosen.iter().map(|w| w.horizon).min().unwrap();
                        let candidate = Witness {
                            node_id: id.clone(),
                            evidence_ids: chosen
                                .iter()
                                .flat_map(|w| w.evidence_ids.iter().cloned())
                                .collect(),
                            horizon,
                            justification_id: Some(rule_id.clone()),
                            premise_ids: chosen.iter().map(|w| w.node_id.clone()).collect(),
                        };
                        let key = |w: &Witness| {
                            (
                                Reverse(w.horizon),
                                w.evidence_ids.len(),
                                w.justification_id.clone(),
                            )
                        };
                        if best.as_ref().is_none_or(|old| key(&candidate) < key(old)) {
                            best = Some(candidate);
                        }
                        Value::Valid { horizon }
                    };
                    result
                        .assurance_by_justification
                        .insert(rule_id.clone(), rule_value);
                }
                let value = best.as_ref().map_or(
                    if unknown_rule {
                        Value::Unknown
                    } else {
                        Value::Invalid
                    },
                    |w| Value::Valid { horizon: w.horizon },
                );
                (value, best)
            };
            result.assurance_by_node.insert(id.clone(), after);
            match &new_witness {
                Some(w) => {
                    result
                        .preferred_witness_by_node
                        .insert(id.clone(), w.clone());
                }
                None => {
                    result.preferred_witness_by_node.remove(&id);
                }
            }
            if before != after || old_witness != new_witness {
                if let Some(rules) = graph.justifications_by_premise().get(&id) {
                    for rule in rules {
                        let conclusion = &graph.justifications()[rule].conclusion;
                        queue.insert((self.ranks[conclusion], conclusion.clone()));
                    }
                }
                changes.push(Change {
                    id,
                    before,
                    after,
                    old_witness,
                    new_witness,
                });
            }
        }
        changes.sort_by(|a, b| a.id.cmp(&b.id));
        (changes, stats)
    }
}
