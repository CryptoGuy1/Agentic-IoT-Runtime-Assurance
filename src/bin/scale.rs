//! Matched full/incremental graph-size sweep, independent of building physics.
use dcra::evidence::{AssuranceStatus as Status, EvidenceAtom};
use dcra::graph::{AssuranceGraph, Justification, Node, NodeKind};
use dcra::runtime::{AssuranceRuntime, EvaluationMode};
use std::fmt::Write;
use std::time::Duration;
fn graph(groups: usize) -> AssuranceGraph {
    let mut nodes = vec![Node::new("policy", NodeKind::Evidence)];
    let mut rules = Vec::new();
    for g in 0..groups {
        let ids: Vec<_> = (0..3).map(|j| format!("g{g:05}.sensor{j}")).collect();
        for id in &ids {
            nodes.push(Node::new(id, NodeKind::Evidence));
        }
        for (name, kind, premises, k) in [
            ("hazard", NodeKind::Derived, ids.clone(), 2),
            (
                "start",
                NodeKind::ActionStart,
                vec![format!("g{g:05}.hazard"), "policy".into()],
                2,
            ),
            ("run", NodeKind::ActionRun, ids[1..].to_vec(), 1),
        ] {
            let id = format!("g{g:05}.{name}");
            nodes.push(Node::new(&id, kind));
            rules.push(Justification {
                justification_id: format!("rule.{id}"),
                conclusion: id,
                premises,
                threshold: k,
            });
        }
    }
    AssuranceGraph::new(nodes, rules).unwrap()
}
fn atom(id: &str, version: u64, now: Duration, status: Status) -> EvidenceAtom {
    EvidenceAtom {
        evidence_id: id.into(),
        evidence_type: "scale-fixture".into(),
        predicate: id.into(),
        source_id: id.into(),
        version,
        observed_at: now,
        expires_at: now + Duration::from_millis(50),
        status,
        payload_hash: None,
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("scale: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut sizes = vec![8, 64, 256];
    let mut updates = 100;
    let mut out = "target/scale-sweep".to_string();
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).ok_or("missing option value")?;
        match args[i].as_str() {
            "--sizes" => sizes = value.split(',').map(str::parse).collect::<Result<_, _>>()?,
            "--updates" => updates = value.parse()?,
            "--out" => out = value.clone(),
            _ => return Err("unknown scale option".into()),
        }
        i += 2;
    }
    if sizes.is_empty()
        || sizes.iter().any(|n| *n == 0 || *n > 4096)
        || updates == 0
        || updates > 10000
    {
        return Err("sizes must be 1..4096 and updates 1..10000".into());
    }
    std::fs::create_dir(&out)?;
    let path = std::path::Path::new(&out);
    let mut csv="groups,mode,time_ns,epoch,N,M,M_e,nodes_recomputed,justifications_recomputed,propagation_ns\n".to_string();
    for groups in &sizes {
        let graph = graph(*groups);
        let mut full = AssuranceRuntime::with_mode(graph.clone(), EvaluationMode::Full);
        let mut inc = AssuranceRuntime::new(graph);
        for runtime in [&mut full, &mut inc] {
            for g in 0..*groups {
                for sensor in 0..3 {
                    runtime.update(atom(
                        &format!("g{g:05}.sensor{sensor}"),
                        1,
                        Duration::ZERO,
                        Status::Valid,
                    ))?;
                }
            }
            runtime.update(atom("policy", 1, Duration::ZERO, Status::Valid))?;
            runtime.enable_work_log();
        }
        for step in 1..=updates {
            let now = Duration::from_millis(step as u64);
            // Local evidence changes plus high fan-out shared-policy changes.
            let id = if step % 5 == 0 {
                "policy".into()
            } else {
                format!("g{:05}.sensor{}", step % groups, step % 3)
            };
            let status = if step % 7 == 0 {
                Status::Unknown
            } else {
                Status::Valid
            };
            let version = inc.evidence()[&id].version + 1;
            for runtime in [&mut full, &mut inc] {
                runtime.advance_to(now)?;
                runtime.update(atom(&id, version, now, status))?;
            }
            if full.evaluation() != inc.evaluation() || full.audit() != inc.audit() {
                return Err(format!("semantic mismatch at groups={groups} step={step}").into());
            }
        }
        for (mode, runtime) in [("Full", full), ("Incremental", inc)] {
            for w in runtime.work_log() {
                writeln!(
                    csv,
                    "{groups},{mode},{},{},{},{},{},{},{},{}",
                    w.at.as_nanos(),
                    w.epoch,
                    w.nodes,
                    w.rules,
                    w.affected_rules,
                    w.stats.nodes_evaluated,
                    w.stats.justifications_evaluated,
                    w.elapsed_ns
                )?;
            }
        }
    }
    std::fs::write(path.join("workload.csv"), csv)?;
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    std::fs::write(
        path.join("manifest.json"),
        format!(
            "{{\"schema\":1,\"groups\":{sizes:?},\"updates\":{updates},\"modes\":[\"Full\",\"Incremental\"],\"equivalent_after_every_operation\":true,\"initialization_measured\":false,\"build_profile\":\"{}\",\"git_commit\":\"{}\",\"fixture\":\"three sensors, threshold hazard, start and run roots per group; shared policy\"}}\n",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            String::from_utf8_lossy(&commit.stdout).trim()
        ),
    )?;
    println!("Scale sweep completed with equal reference/incremental semantics: {out}");
    Ok(())
}
