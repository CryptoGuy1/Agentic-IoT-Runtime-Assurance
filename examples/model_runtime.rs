//! A deterministic walkthrough of the optional model/runtime integration.
#[path = "support/actions.rs"]
#[allow(dead_code)]
mod support;
use dcra::actions::*;
use dcra::evidence::AssuranceStatus as Status;
use dcra::model_v1::*;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
fn p(v: f64) -> Interval {
    Interval::new(v, v).unwrap()
}
fn configured(door: f64, hazard: f64, may_drop: bool) -> ModelRuntime {
    let (graph, mut definitions) =
        support::configuration(Interruptibility::Preemptible, RecoveryType::Reversible);
    definitions[0].min_duration = TICK;
    let runtime = ActionRuntime::new(graph, definitions).unwrap();
    let state = StateBox {
        at: Duration::ZERO,
        hazard: p(hazard),
        pressure: p(0.0),
        fan: p(0.0),
        door: p(door),
        occupied: BoolSet::False,
        alarm: BoolSet::False,
        fan_failed: BoolSet::False,
        door_jammed: BoolSet::False,
        response_required: if hazard >= 0.6 {
            BoolSet::True
        } else {
            BoolSet::False
        },
    };
    let estimator = Estimator::new(
        state,
        Inputs::default(),
        Assumptions {
            source: p(0.0),
            disturbance_scale: 0.0,
            faults_may_start: false,
            occupancy_stable_until: Duration::from_secs(100),
            ..Assumptions::default()
        },
    )
    .unwrap();
    let binding = ModelBinding {
        action: Action::FanStart,
        recovery: Action::FanStop,
        max_delay: Duration::ZERO,
        may_drop,
    };
    let mut runtime = ModelRuntime::new(
        runtime,
        estimator,
        BTreeMap::from([("fan".into(), binding.clone()), ("follow".into(), binding)]),
    )
    .unwrap();
    for id in ["start", "run_a", "commit"] {
        runtime
            .update_permission(support::observation(id, Status::Valid, 1, 100))
            .unwrap();
    }
    runtime
        .register_plan(Plan {
            plan_id: "review".into(),
            version: 1,
            actions: BTreeMap::from([("a".into(), "fan".into())]),
            precedence: BTreeSet::new(),
            created_at: Duration::ZERO,
        })
        .unwrap();
    runtime
}
fn release(runtime: &mut ModelRuntime) -> PhysicalCommand {
    let token = runtime.request_lease("a", ActionPhase::Start).unwrap();
    runtime.dispatch("a", ActionPhase::Start, &token).unwrap();
    runtime.take_command().unwrap().unwrap()
}
fn main() {
    println!("MODEL/RUNTIME REVIEW — deterministic synthetic fixtures");
    println!(
        "Assumptions: exact initial values, no disturbances or new faults, guaranteed delivery except case 4. These are software checks, not measured hardware outcomes.\n"
    );
    let mut safe = configured(0.0, 0.0, false);
    release(&mut safe);
    safe.advance_to(TICK).unwrap();
    println!(
        "1. Safe fan command released; without an outcome report: {:?}.",
        safe.runtime().instances()["a"].state
    );
    let mut running = configured(1.0, 0.0, false);
    release(&mut running);
    while running.runtime().instances()["a"].state == ActionState::Executing {
        running.advance_to(running.runtime().now() + TICK).unwrap();
    }
    let command = running.take_command().unwrap().unwrap();
    println!(
        "2. Continuing pressure risk at {:.1}s: {:?}; checked recovery released: {:?}.",
        running.runtime().now().as_secs_f64(),
        running.runtime().instances()["a"].state,
        command.model_command.action
    );
    println!(
        "   Case 3 supplies an early synthetic controller report to exercise the lifecycle boundary; it does not establish physical target achievement."
    );
    let mut completed = configured(1.0, 0.0, false);
    let command = release(&mut completed);
    completed.advance_to(TICK).unwrap();
    completed
        .submit_result(support::report(
            &command.runtime_command.unwrap().command_id,
            1,
            TICK,
            Some(true),
            true,
            Some(Status::Valid),
        ))
        .unwrap();
    loop {
        completed
            .advance_to(completed.runtime().now() + TICK)
            .unwrap();
        if let Some(command) = completed.take_command().unwrap() {
            println!(
                "3. Task {:?}; persistent fan effect monitored; separate {:?} recovery released at {:.1}s.",
                completed.runtime().instances()["a"].state,
                command.model_command.action,
                completed.runtime().now().as_secs_f64()
            );
            break;
        }
    }
    let mut loss = configured(0.0, 0.0, true);
    assert!(loss.request_lease("a", ActionPhase::Start).is_err());
    println!(
        "4. Possible recovery-command loss: start withheld because the recovery target is not guaranteed."
    );
    let mut hazard = configured(0.0, 0.8, false);
    assert!(hazard.request_lease("a", ActionPhase::Start).is_err());
    println!(
        "5. Active hazard: start withheld because the configured FanStop recovery is not supported. A different justified recovery policy is needed."
    );
    println!("\nPredictions never create acknowledgements or physical outcome evidence.");
}
