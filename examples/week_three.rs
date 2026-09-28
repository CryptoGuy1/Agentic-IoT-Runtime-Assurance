//! Run with: cargo run --offline --example week_three
#[path = "support/actions.rs"]
mod fixture;
use dcra::actions::*;
use dcra::evidence::AssuranceStatus as Status;
use dcra::runtime::EvaluationMode;
use fixture::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime(
        EvaluationMode::Incremental,
        Interruptibility::Preemptible,
        RecoveryType::Reversible,
    );
    println!("C5: an observation refresh invalidates an unused start lease.");
    let old = runtime.request_lease("a", ActionPhase::Start)?;
    runtime.update(observation("start", Status::Valid, 2, 100))?;
    let rejected = runtime.dispatch("a", ActionPhase::Start, &old);
    println!("  Old lease dispatch: {rejected:?}");
    assert!(rejected.is_err());
    println!("  Commands queued: {}", runtime.commands().len());
    let command = start(&mut runtime);
    println!(
        "  Fresh authority accepted: state={:?}, command={command}",
        runtime.instances()["a"].state
    );
    runtime.take_command().unwrap();
    runtime.advance_to(secs(2))?;
    println!("\nC10: a command was sent, but acknowledgement is unknown.");
    runtime.submit_result(report(
        &command,
        1,
        runtime.now(),
        None,
        true,
        Some(Status::Valid),
    ))?;
    println!(
        "  Outcome: {:?}",
        runtime.assurance().evaluation().assurance_by_node["a.outcome"]
    );
    println!(
        "  Successor request: {:?}",
        runtime.request_lease("b", ActionPhase::Start)
    );
    println!("\nC7: running evidence expires while the clock advances to 5 seconds.");
    runtime.update(observation("run_a", Status::Valid, 2, 3))?;
    runtime.advance_to(secs(5))?;
    let abort = runtime.take_command().unwrap();
    println!(
        "  State={:?}; {:?} queued at {:?}",
        runtime.instances()["a"].state,
        abort.kind,
        abort.issued_at
    );
    runtime.submit_result(report(
        &abort.command_id,
        1,
        runtime.now(),
        Some(true),
        true,
        None,
    ))?;
    println!(
        "  Fake controller confirms completion of abort: {:?}",
        runtime.instances()["a"].state
    );
    println!(
        "\nThese are local lifecycle checks with simulated responses, not physical safety certification."
    );
    Ok(())
}
