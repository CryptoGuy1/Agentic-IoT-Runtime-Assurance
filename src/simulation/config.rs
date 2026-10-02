use super::model::*;
use crate::actions::*;
use crate::graph::{AssuranceGraph, Justification, Node, NodeKind};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Baseline {
    B0,
    B1,
    B2,
    B3,
    B4,
    B5,
}
impl std::str::FromStr for Baseline {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "B0" => Ok(Self::B0),
            "B1" => Ok(Self::B1),
            "B2" => Ok(Self::B2),
            "B3" => Ok(Self::B3),
            "B4" => Ok(Self::B4),
            "B5" => Ok(Self::B5),
            _ => Err("baseline must be B0–B5".into()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationConfig {
    pub scenario: String,
    pub baseline: Baseline,
    pub seed: u64,
}
impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            scenario: "S1".into(),
            baseline: Baseline::B5,
            seed: 42,
        }
    }
}
pub fn configuration(baseline: Baseline) -> (AssuranceGraph, Vec<ActionDefinition>) {
    experiment_configuration(baseline, super::Ablation::None)
}
pub fn experiment_configuration(
    baseline: Baseline,
    ablation: super::Ablation,
) -> (AssuranceGraph, Vec<ActionDefinition>) {
    let mut nodes: Vec<_> = SENSORS
        .iter()
        .chain(["always"].iter())
        .map(|id| Node::new(*id, NodeKind::Evidence))
        .collect();
    let mut rules = Vec::new();
    fn add(
        nodes: &mut Vec<Node>,
        rules: &mut Vec<Justification>,
        id: &str,
        kind: NodeKind,
        p: Vec<String>,
        threshold: usize,
    ) {
        nodes.push(Node::new(id, kind));
        rules.push(Justification {
            justification_id: format!("rule.{id}"),
            premises: p,
            threshold,
            conclusion: id.into(),
        });
    }
    add(
        &mut nodes,
        &mut rules,
        "hazard",
        NodeKind::Derived,
        ["s1", "s2", "thermal"].map(String::from).to_vec(),
        if baseline == Baseline::B2 || ablation == super::Ablation::NoAlternatives {
            3
        } else {
            2
        },
    );
    let mut defs = Vec::new();
    for a in ACTIONS {
        for suffix in ["safe", "ack", "done"] {
            nodes.push(Node::new(format!("{a}.{suffix}"), NodeKind::Evidence));
        }
        add(
            &mut nodes,
            &mut rules,
            &format!("{a}.physical"),
            NodeKind::PhysicalSafety,
            vec![format!("{a}.safe")],
            1,
        );
        let mut start: Vec<String> = match a {
            "alarm" | "light" => vec!["hazard".into()],
            "fan" => vec!["hazard".into(), "fan_healthy".into(), "fan.physical".into()],
            "door" | "damper" => vec![format!("{a}.physical")],
            "arm" => vec![],
            "discharge" => vec!["suppression_armed".into()],
            _ => unreachable!(),
        };
        start.extend(["permission".into(), "agent_available".into()]);
        let full_start = start.clone();
        let override_contract = |original: Vec<String>, commit: bool| match baseline {
            Baseline::B0 => vec!["always".into()],
            Baseline::B1 => vec!["permission".into()],
            Baseline::B3 => [
                "s1",
                "s2",
                "thermal",
                "fan_healthy",
                "pressure_safe",
                "occupancy_clear",
                "egress_clear",
                "path_healthy",
                "operator_approved",
                "agent_available",
                "permission",
            ]
            .map(String::from)
            .to_vec(),
            Baseline::B4 => {
                if commit {
                    vec!["always".into()]
                } else {
                    vec![format!("{a}.physical")]
                }
            }
            _ => original,
        };
        start = override_contract(start, false);
        let n = start.len();
        add(
            &mut nodes,
            &mut rules,
            &format!("{a}.start"),
            NodeKind::ActionStart,
            start,
            n,
        );
        let run = match a {
            "fan" => Some(vec!["fan_healthy".into(), "pressure_safe".into()]),
            "door" => Some(vec!["agent_available".into(), "egress_clear".into()]),
            _ => None,
        };
        let run = if baseline == Baseline::B2 {
            let mut p = full_start;
            if let Some(r) = run {
                p.extend(r);
            }
            p.sort();
            p.dedup();
            Some(p)
        } else if baseline == Baseline::B5 {
            run
        } else {
            None
        };
        if let Some(ref p) = run {
            add(
                &mut nodes,
                &mut rules,
                &format!("{a}.run"),
                NodeKind::ActionRun,
                p.clone(),
                p.len(),
            );
        }
        let commit = matches!(a, "door" | "discharge");
        if commit {
            let p = if a == "discharge" {
                vec![
                    "hazard".into(),
                    "occupancy_clear".into(),
                    "alarm_active".into(),
                    "path_healthy".into(),
                    "operator_approved".into(),
                    "discharge.physical".into(),
                    "permission".into(),
                    "agent_available".into(),
                ]
            } else {
                vec!["door.physical".into(), "agent_available".into()]
            };
            let p = override_contract(p, true);
            let n = p.len();
            add(
                &mut nodes,
                &mut rules,
                &format!("{a}.commit"),
                NodeKind::ActionCommit,
                p,
                n,
            );
        }
        add(
            &mut nodes,
            &mut rules,
            &format!("{a}.outcome"),
            NodeKind::ActionOutcome,
            vec![format!("{a}.ack"), format!("{a}.done")],
            2,
        );
        defs.push(ActionDefinition {
            action_id: a.into(),
            start_contract: format!("{a}.start"),
            run_contract: run.map(|_| format!("{a}.run")),
            commit_contract: commit.then(|| format!("{a}.commit")),
            outcome_contract: format!("{a}.outcome"),
            min_duration: duration(a),
            max_duration: duration(a) + Duration::from_secs(2),
            interruptibility: match a {
                "door" | "damper" => Interruptibility::CompletionSafe,
                "discharge" => Interruptibility::Irreversible,
                _ => Interruptibility::Preemptible,
            },
            recovery_type: if a == "discharge" {
                RecoveryType::Irreversible
            } else {
                RecoveryType::Reversible
            },
            resources_read: BTreeSet::new(),
            resources_written: BTreeSet::from([a.into()]),
            max_start_lease: Duration::from_secs(1),
            controllers: ControllerCapabilities {
                safe_abort: true,
                certified_complete: true,
                forward_mitigation: true,
                compensate: false,
            },
            outcomes: OutcomeBindings {
                acknowledgement: format!("{a}.ack"),
                observed: BTreeMap::from([("done".into(), format!("{a}.done"))]),
                max_age: Duration::from_secs(30),
            },
        });
    }
    if ablation == super::Ablation::NoRun {
        for d in &mut defs {
            d.run_contract = None;
        }
    }
    if ablation == super::Ablation::NoCommit {
        for r in &mut rules {
            if r.conclusion.ends_with(".commit") {
                r.premises = vec!["always".into()];
                r.threshold = 1;
            }
        }
    }
    if ablation == super::Ablation::NoPhysical {
        for r in &mut rules {
            if r.conclusion.ends_with(".physical") {
                r.premises = vec!["always".into()];
                r.threshold = 1;
            }
        }
    }
    (
        AssuranceGraph::new(nodes, rules).expect("built-in graph"),
        defs,
    )
}
/// Scripted planner input is an observation estimate and immutable execution history.
pub trait Planner {
    fn propose(
        &self,
        scenario: &str,
        state: &StateEstimate,
        history: &[ActionInstance],
        version: u64,
    ) -> Plan;
}
pub struct ScriptedPlanner;
impl Planner for ScriptedPlanner {
    fn propose(
        &self,
        scenario: &str,
        _state: &StateEstimate,
        history: &[ActionInstance],
        version: u64,
    ) -> Plan {
        let names: Vec<&str> = match scenario {
            "S3" => vec!["alarm", "arm", "discharge"],
            "S2" | "C13" | "C14" => vec!["fan"],
            "C11" => vec!["damper", "fan"],
            "C12" => vec!["alarm", "light"],
            "C15" => vec!["door"],
            "C16" => vec!["alarm", "door", "fan", "arm"],
            _ => vec!["alarm", "fan", "door"],
        };
        let mut actions = BTreeMap::new();
        let mut ids = Vec::new();
        for (i, a) in names.iter().enumerate() {
            if let Some(old) = history
                .iter()
                .find(|h| h.action_id == *a && h.started_at.is_some())
            {
                ids.push(old.instance_id.clone());
            } else {
                let id = format!("v{version}.{i}.{a}");
                ids.push(id.clone());
                actions.insert(id, (*a).into());
            }
        }
        let mut precedence = BTreeSet::new();
        if scenario == "S3" {
            precedence.insert((ids[0].clone(), ids[2].clone()));
            precedence.insert((ids[1].clone(), ids[2].clone()));
        } else if !matches!(scenario, "C11" | "C12") {
            for pair in ids.windows(2) {
                if actions.contains_key(&pair[1]) {
                    precedence.insert((pair[0].clone(), pair[1].clone()));
                }
            }
        }
        Plan {
            plan_id: "building".into(),
            version,
            actions,
            precedence,
            created_at: Duration::ZERO,
        }
    }
}
