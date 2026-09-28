//! Run with: cargo run --offline --example week_two
use dcra::evidence::{AssuranceStatus as Status, EvidenceAtom};
use dcra::graph::{AssuranceGraph, Justification, Node, NodeKind};
use dcra::runtime::{AssuranceRuntime, EvaluationMode};
use std::time::Duration;

fn observation(id: &str, status: Status, version: u64) -> EvidenceAtom {
    EvidenceAtom {
        evidence_id: id.into(),
        evidence_type: "sensor".into(),
        predicate: "condition".into(),
        source_id: id.into(),
        version,
        observed_at: Duration::ZERO,
        expires_at: Duration::from_secs(10),
        status,
        payload_hash: None,
    }
}
fn rule(id: &str, inputs: &[&str], threshold: usize, output: &str) -> Justification {
    Justification {
        justification_id: id.into(),
        premises: inputs.iter().map(|s| (*s).into()).collect(),
        threshold,
        conclusion: output.into(),
    }
}
fn compare(full: &AssuranceRuntime, incremental: &AssuranceRuntime) {
    assert_eq!(full.evaluation(), incremental.evaluation());
    assert_eq!(full.audit(), incremental.audit());
    println!("  Identical conclusions, witnesses and audit history.");
    for runtime in [full, incremental] {
        let work = runtime.operation_stats();
        println!(
            "  {:?}: nodes evaluated = {}, rules evaluated = {}",
            runtime.mode(),
            work.nodes_evaluated,
            work.justifications_evaluated
        );
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = AssuranceGraph::new(
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
    )?;
    let mut full = AssuranceRuntime::with_mode(graph.clone(), EvaluationMode::Full);
    let mut incremental = AssuranceRuntime::new(graph);
    for id in ["s1", "s2", "thermal", "occupancy"] {
        for runtime in [&mut full, &mut incremental] {
            runtime.update(observation(id, Status::Valid, 1))?;
        }
    }
    println!("C1/C3: S2 becomes unknown; a different sensor pair supports the hazard.");
    for runtime in [&mut full, &mut incremental] {
        runtime.update(observation("s2", Status::Unknown, 2))?;
    }
    compare(&full, &incremental);
    println!(
        "  Fan supported by {:?}",
        incremental.evaluation().preferred_witness_by_node["fan"].evidence_ids
    );
    println!("\nC4: Occupancy becomes unknown; only the door branch needs checking.");
    for runtime in [&mut full, &mut incremental] {
        runtime.update(observation("occupancy", Status::Unknown, 2))?;
    }
    compare(&full, &incremental);
    for id in ["fan", "alarm", "door"] {
        println!(
            "  {id}: {:?}",
            incremental.evaluation().assurance_by_node[id]
        );
    }
    println!("\nC2: Advance to 11 seconds; remaining sensor evidence expires at 10.");
    for runtime in [&mut full, &mut incremental] {
        runtime.advance_to(Duration::from_secs(11))?;
    }
    compare(&full, &incremental);
    println!(
        "  Fan: {:?}; S1 version still {}",
        incremental.evaluation().assurance_by_node["fan"],
        incremental.evidence()["s1"].version
    );
    println!("\nCounts measure evaluation work, not end-to-end speed or a timing guarantee.");
    Ok(())
}
