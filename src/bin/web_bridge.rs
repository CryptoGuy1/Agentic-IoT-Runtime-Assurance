//! Line-based adapter for the local meeting demo. Never accepts arbitrary commands.
use dcra::{runtime::EvaluationMode, simulation::*};
use std::io::{self, BufRead, Write};
fn new_sim(preset: &str) -> Result<Simulation, String> {
    let profile = match preset {
        "pressure" => "none",
        "late" => "stale-lease",
        _ => return Err("Unknown preset".into()),
    };
    Simulation::with_experiment(
        SimulationConfig {
            scenario: "S1".into(),
            baseline: Baseline::B5,
            seed: 42,
        },
        EvaluationMode::Incremental,
        NetworkConfig::profile(profile)?,
        Ablation::None,
    )
}
fn allowed_interaction(line: &str) -> bool {
    let parts: Vec<_> = line.split_whitespace().collect();
    match parts.as_slice() {
        ["fault", id, op] => {
            [
                "s1",
                "s2",
                "thermal",
                "pressure_safe",
                "fan_healthy",
                "occupancy_clear",
                "egress_clear",
                "agent_available",
            ]
            .contains(id)
                && ["drop", "restore"].contains(op)
        }
        ["plant", field, value] => matches!(
            (*field, *value),
            ("hazard", "on" | "off")
                | ("occupancy", "occupied" | "clear")
                | ("egress", "blocked" | "clear")
                | ("pressure", "20" | "100")
        ),
        _ => false,
    }
}
fn main() -> Result<(), String> {
    let mut preset = "pressure".to_string();
    let mut sim = new_sim(&preset)?;
    let mut ai = false;
    let mut revision = 0u64;
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let result = match line.as_str() {
            "state" | "planning" => Ok(String::new()),
            "reset pressure" | "reset late" | "reset-ai pressure" | "reset-ai late" => {
                let next = line.split_whitespace().nth(1).unwrap();
                new_sim(next).and_then(|mut fresh| {
                    let next_ai = line.starts_with("reset-ai ");
                    if next_ai {
                        fresh.apply_planner_sequence(&[])?;
                        fresh.command("advance 0ms")?;
                    }
                    sim = fresh;
                    ai = next_ai;
                    preset = next.into();
                    Ok(String::new())
                })
            }
            "advance 200ms"
            | "advance 500ms"
            | "advance 1s"
            | "advance 1200ms"
            | "step"
            | "fault pressure_safe drop"
            | "fault pressure_safe restore" => sim.command(&line),
            _ if line.starts_with("plan ") => {
                let words: Vec<_> = line.split_whitespace().collect();
                if !ai || words.len() != 3 {
                    Err("Plan requires AI mode, revision and action list".into())
                } else if words[1].parse::<u64>().ok() != Some(revision) {
                    Err("Planning state changed; generate a fresh proposal".into())
                } else {
                    let names: Vec<String> = if words[2] == "-" {
                        vec![]
                    } else {
                        words[2].split(',').map(String::from).collect()
                    };
                    sim.apply_planner_sequence(&names).map(|()| String::new())
                }
            }
            _ if allowed_interaction(&line) => sim.command(&line),
            _ => Err("Control not allowed".into()),
        };
        let error = result.err();
        if error.is_none() && !matches!(line.as_str(), "state" | "planning") {
            revision += 1;
        }
        println!(
            "{{\"ok\":{},\"error\":{},\"state\":{},\"planner_mode\":{},\"revision\":{},\"context\":{}}}",
            error.is_none(),
            error.as_ref().map_or("null".into(), |s| sim_json_quote(s)),
            sim.web_snapshot_json(&preset),
            sim_json_quote(if ai { "ai" } else { "scripted" }),
            revision,
            if line == "planning" {
                sim.planner_context_json()
            } else {
                "null".into()
            }
        );
        io::stdout().flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
