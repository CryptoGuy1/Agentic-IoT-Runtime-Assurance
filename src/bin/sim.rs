use dcra::simulation::*;
use std::io::{self, Write};
fn main() {
    if let Err(e) = run() {
        eprintln!("sim: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut config = SimulationConfig::default();
    let mut network = NetworkConfig::default();
    let mut ablation = Ablation::None;
    let mut manifest = None;
    let mut interactive = false;
    let mut output = None;
    let mut replay = None;
    let mut until = "10s".to_string();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--help" => {
                println!(
                    "sim [--scenario S1..S5|C11..C16] [--baseline B0..B5] [--seed N] [--until 10s] [--interactive] [--replay FILE] [--out NEW_DIRECTORY] [--network PROFILE] [--ablation NAME] [--manifest FILE]"
                );
                return Ok(());
            }
            "--interactive" => interactive = true,
            "--scenario" | "--baseline" | "--seed" | "--out" | "--replay" | "--until"
            | "--network" | "--ablation" | "--manifest" => {
                i += 1;
                let value = args.get(i).ok_or("option requires a value")?;
                match arg.as_str() {
                    "--scenario" => config.scenario = value.clone(),
                    "--baseline" => config.baseline = value.parse()?,
                    "--seed" => config.seed = value.parse().map_err(|_| "invalid seed")?,
                    "--out" => output = Some(value.clone()),
                    "--replay" => replay = Some(value.clone()),
                    "--until" => until = value.clone(),
                    "--network" => network = NetworkConfig::profile(value)?,
                    "--ablation" => ablation = value.parse()?,
                    "--manifest" => manifest = Some(value.clone()),
                    _ => unreachable!(),
                }
            }
            _ => return Err(format!("unknown option {arg}")),
        }
        i += 1;
    }
    if replay.is_some() && manifest.is_some() {
        return Err("choose --manifest or --replay".into());
    }
    let mut sim = if let Some(path) = manifest.as_ref() {
        Simulation::replay_manifest(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)?
    } else if let Some(path) = replay.as_ref() {
        Simulation::replay(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)?
    } else {
        Simulation::with_experiment(
            config,
            dcra::runtime::EvaluationMode::Incremental,
            network,
            ablation,
        )?
    };
    if interactive {
        println!(
            "Virtual time is paused between commands. Type help. No real devices are connected."
        );
        loop {
            print!("sim> ");
            io::stdout().flush().map_err(|e| e.to_string())?;
            let mut line = String::new();
            if io::stdin()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?
                == 0
            {
                break;
            }
            let line = line.trim();
            if line == "quit" {
                break;
            }
            if line.is_empty() {
                continue;
            }
            if let Some(path) = line.strip_prefix("save ") {
                match sim.save(path.trim()) {
                    Ok(()) => println!("Saved {path}"),
                    Err(e) => eprintln!("{e}"),
                };
                continue;
            }
            match sim.command(line) {
                Ok(s) => println!("{s}"),
                Err(e) => eprintln!("{e}"),
            }
        }
    } else if replay.is_none() && manifest.is_none() {
        sim.command(&format!("run {until}"))?;
    }
    println!("{}\n{}", sim.status(), sim.metrics_json());
    if let Some(path) = output {
        sim.save(&path)?;
        println!("Experiment written to {path}");
    }
    Ok(())
}
