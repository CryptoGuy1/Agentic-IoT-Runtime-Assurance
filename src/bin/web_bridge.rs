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
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let result = match line.as_str() {
            "state" => Ok(String::new()),
            "reset pressure" | "reset late" => {
                let next = line.split_whitespace().nth(1).unwrap();
                new_sim(next).map(|fresh| {
                    sim = fresh;
                    preset = next.into();
                    String::new()
                })
            }
            "advance 200ms"
            | "advance 500ms"
            | "advance 1s"
            | "advance 1200ms"
            | "step"
            | "fault pressure_safe drop"
            | "fault pressure_safe restore" => sim.command(&line),
            _ if allowed_interaction(&line) => sim.command(&line),
            _ => Err("Control not allowed".into()),
        };
        let error = result.err();
        println!(
            "{{\"ok\":{},\"error\":{},\"state\":{}}}",
            error.is_none(),
            error.as_ref().map_or("null".into(), |s| sim_json_quote(s)),
            sim.web_snapshot_json(&preset)
        );
        io::stdout().flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
