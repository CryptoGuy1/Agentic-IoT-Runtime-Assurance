use dcra::model_v1::{SCENARIOS, run_experiment};
use std::{fs, path::PathBuf};

fn run() -> Result<(), String> {
    let mut scenario = "normal".to_string();
    let mut seed = 42;
    let mut out: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!(
                "model_v1 --scenario NAME --seed N --out NEW_DIRECTORY\nScenarios: {}",
                SCENARIOS.join(", ")
            );
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {arg}"))?;
        match arg.as_str() {
            "--scenario" => scenario = value,
            "--seed" => {
                seed = value
                    .parse()
                    .map_err(|_| "seed must be an unsigned integer")?
            }
            "--out" => out = Some(value.into()),
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    let out =
        out.ok_or("provide --out NEW_DIRECTORY; existing directories are never overwritten")?;
    if out.exists() {
        return Err("output directory already exists; choose a new directory".into());
    }
    let report = run_experiment(&scenario, seed)?;
    // Atomic directory reservation also protects against another process creating it.
    fs::create_dir(&out).map_err(|e| format!("cannot create output directory: {e}"))?;
    fs::write(out.join("results.json"), report.json).map_err(|e| e.to_string())?;
    fs::write(out.join("report.md"), report.markdown).map_err(|e| e.to_string())?;
    println!(
        "Model v1: {:?}; containment {}/{} ticks. Results: {}",
        report.decision,
        report.covered_ticks,
        report.total_ticks,
        out.display()
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("model_v1: {e}");
        std::process::exit(1);
    }
}
