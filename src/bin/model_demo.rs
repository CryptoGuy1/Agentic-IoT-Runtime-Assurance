use dcra::model_v1::{
    CLOSED_LOOP_SCENARIOS, RefinementPolicy, RunOptions, run_closed_loop_with_options,
};
use std::{fs, path::PathBuf};
fn run() -> Result<(), String> {
    let mut scenario = "normal".to_string();
    let mut seed = 42;
    let mut options = RunOptions::default();
    let mut out: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!(
                "model_demo --scenario NAME --seed N --out NEW_DIRECTORY [--policy B0..B6] [--oracle-v2] [--autonomous] [--broader-recovery]\n--policy selects frozen benchmark timing; --oracle-v2 emits client-specified independent candidate labels.\nScenarios: {}",
                CLOSED_LOOP_SCENARIOS.join(", ")
            );
            return Ok(());
        }
        if arg == "--autonomous" {
            options.autonomous = true;
            continue;
        }
        if arg == "--broader-recovery" {
            options.broader_recovery = true;
            continue;
        }
        if arg == "--oracle-v2" {
            options.oracle_v2 = true;
            options.benchmark = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {arg}"))?;
        match arg.as_str() {
            "--scenario" => scenario = value,
            "--policy" => {
                options.policy = RefinementPolicy::parse(&value)?;
                options.benchmark = true;
            }
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
    let report = run_closed_loop_with_options(&scenario, seed, options)?;
    fs::create_dir(&out).map_err(|e| format!("cannot create output directory: {e}"))?;
    fs::write(out.join("results.json"), report.json).map_err(|e| e.to_string())?;
    fs::write(out.join("report.md"), report.markdown).map_err(|e| e.to_string())?;
    fs::write(out.join("events.jsonl"), report.events_jsonl).map_err(|e| e.to_string())?;
    println!(
        "{scenario}: {:?}; {} physical commands; containment {}/{}. Results: {}",
        report.final_state,
        report.score.commands.len(),
        report.contained_ticks,
        report.total_ticks,
        out.display()
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("model_demo: {e}");
        std::process::exit(1);
    }
}
