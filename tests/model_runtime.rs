#[path = "../examples/support/actions.rs"]
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
fn fixture(
    action: Action,
    recovery: Action,
    delay: u64,
    drop: bool,
    door: f64,
    hazard: f64,
) -> ModelRuntime {
    fixture_with_class(
        action,
        recovery,
        delay,
        drop,
        door,
        hazard,
        Interruptibility::Preemptible,
    )
}
fn fixture_with_class(
    action: Action,
    recovery: Action,
    delay: u64,
    drop: bool,
    door: f64,
    hazard: f64,
    class: Interruptibility,
) -> ModelRuntime {
    let (graph, mut defs) = support::configuration(
        class,
        if class == Interruptibility::Irreversible {
            RecoveryType::Irreversible
        } else {
            RecoveryType::Reversible
        },
    );
    defs[0].min_duration = TICK;
    let runtime = ActionRuntime::new(graph, defs).unwrap();
    let mut state = StateBox::unknown();
    state.hazard = p(hazard);
    state.fan = p(0.0);
    state.pressure = p(0.0);
    state.door = p(door);
    state.occupied = BoolSet::False;
    state.alarm = BoolSet::False;
    state.fan_failed = BoolSet::False;
    state.door_jammed = BoolSet::False;
    state.response_required = if hazard >= 0.6 {
        BoolSet::True
    } else {
        BoolSet::False
    };
    let estimator = Estimator::new(
        state,
        Inputs::default(),
        Assumptions {
            disturbance_scale: 0.0,
            source: p(0.0),
            faults_may_start: false,
            occupancy_stable_until: Duration::from_secs(100),
            ..Assumptions::default()
        },
    )
    .unwrap();
    let binding = ModelBinding {
        action,
        recovery,
        max_delay: Duration::from_millis(delay),
        may_drop: drop,
    };
    let mut r = ModelRuntime::new(
        runtime,
        estimator,
        BTreeMap::from([("fan".into(), binding.clone()), ("follow".into(), binding)]),
    )
    .unwrap();
    for id in ["start", "run_a", "commit"] {
        r.update_permission(support::observation(id, Status::Valid, 1, 100))
            .unwrap();
    }
    r.register_plan(Plan {
        plan_id: "plan".into(),
        version: 1,
        actions: BTreeMap::from([("a".into(), "fan".into())]),
        precedence: BTreeSet::new(),
        created_at: Duration::ZERO,
    })
    .unwrap();
    r
}
fn start(r: &mut ModelRuntime) -> PhysicalCommand {
    let lease = r.request_lease("a", ActionPhase::Start).unwrap();
    r.dispatch("a", ActionPhase::Start, &lease).unwrap();
    r.take_command().unwrap().unwrap()
}
#[test]
fn permission_prediction_and_outcome_are_separate() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 0.0, 0.0);
    let command = start(&mut r);
    assert_eq!(r.runtime().instances()["a"].state, ActionState::Executing);
    r.advance_to(Duration::from_secs(2)).unwrap();
    assert_ne!(r.runtime().instances()["a"].state, ActionState::Completed);
    r.submit_result(support::report(
        &command.runtime_command.unwrap().command_id,
        1,
        Duration::from_secs(2),
        Some(true),
        true,
        Some(Status::Valid),
    ))
    .unwrap();
    assert_eq!(r.runtime().instances()["a"].state, ActionState::Completed);
    let count = r.audit().len();
    r.advance_to(Duration::from_secs(3)).unwrap();
    assert!(r.audit().len() > count);
    assert!(r.audit().iter().any(|a| a.phase == "persistent-monitor"));
}
#[test]
fn supported_model_does_not_replace_permission() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 0.0, 0.0);
    r.update_permission(support::observation("start", Status::Invalid, 2, 100))
        .unwrap();
    assert!(matches!(
        r.request_lease("a", ActionPhase::Start),
        Err(IntegratedError::Runtime(ActionError::ContractUnavailable(
            _
        )))
    ));
    assert!(r.estimator().commands().is_empty());
}
#[test]
fn unsafe_recovery_is_not_assumed_safe() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 0.0, 0.8);
    assert!(matches!(
        r.request_lease("a", ActionPhase::Start),
        Err(IntegratedError::Blocked(_))
    ));
    assert_eq!(r.audit().last().unwrap().phase, "recovery-feasibility");
}
#[test]
fn lost_recovery_cannot_guarantee_target() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, true, 0.0, 0.0);
    assert!(matches!(
        r.request_lease("a", ActionPhase::Start),
        Err(IntegratedError::Blocked(_))
    ));
    assert!(
        r.audit()
            .last()
            .unwrap()
            .check
            .reasons
            .iter()
            .any(|s| s.contains("target"))
    );
}
#[test]
fn delayed_recovery_misses_deadline() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 6000, false, 0.0, 0.0);
    assert!(matches!(
        r.request_lease("a", ActionPhase::Start),
        Err(IntegratedError::Blocked(_))
    ));
}
#[test]
fn running_risk_requests_checked_recovery() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 1.0, 0.0);
    start(&mut r);
    for n in 1..=100 {
        r.advance_to(Duration::from_millis(n * 100)).unwrap();
        if r.runtime().instances()["a"].state == ActionState::Aborting {
            break;
        }
    }
    assert_eq!(r.runtime().instances()["a"].state, ActionState::Aborting);
    let recovery = r.take_command().unwrap().unwrap();
    assert_eq!(recovery.model_command.action, Action::FanStop);
    assert_eq!(
        recovery.runtime_command.unwrap().kind,
        CommandKind::SafeAbort
    );
}
#[test]
fn completed_effect_can_trigger_separate_recovery() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 1.0, 0.0);
    let command = start(&mut r);
    // Report completion at the allowed minimum while preserving a continuing fan effect.
    r.advance_to(TICK).unwrap();
    r.submit_result(support::report(
        &command.runtime_command.unwrap().command_id,
        1,
        TICK,
        Some(true),
        true,
        Some(Status::Valid),
    ))
    .unwrap();
    assert_eq!(r.runtime().instances()["a"].state, ActionState::Completed);
    let recovery = loop {
        let next = r.runtime().now() + TICK;
        assert!(next < Duration::from_secs(10));
        r.advance_to(next).unwrap();
        if let Some(c) = r.take_command().unwrap() {
            break c;
        }
    };
    assert_eq!(recovery.model_command.action, Action::FanStop);
    assert!(recovery.runtime_command.is_none());
    assert_eq!(r.runtime().instances()["a"].state, ActionState::Completed);
}
#[test]
fn recheck_at_dispatch_blocks_changed_model() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 0.0, 0.0);
    let lease = r.request_lease("a", ActionPhase::Start).unwrap();
    r.observe(Observation {
        id: "bad".into(),
        source: "sensor".into(),
        variable: Variable::Pressure,
        value: Reading::Number(0.9),
        error: Some(0.0),
        generated_at: Duration::ZERO,
        received_at: Duration::ZERO,
        max_age: Duration::from_secs(1),
        version: 1,
        quality: Quality::Trusted,
    })
    .unwrap();
    assert!(r.estimator().inconsistent());
    assert!(matches!(
        r.dispatch("a", ActionPhase::Start, &lease),
        Err(IntegratedError::Blocked(_))
    ));
    assert!(r.estimator().commands().is_empty());
}
#[test]
fn unreleased_command_cannot_supply_outcome() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 0.0, 0.0);
    let lease = r.request_lease("a", ActionPhase::Start).unwrap();
    let command = r.dispatch("a", ActionPhase::Start, &lease).unwrap();
    assert!(matches!(
        r.submit_result(support::report(
            &command,
            1,
            Duration::ZERO,
            Some(true),
            true,
            Some(Status::Valid)
        )),
        Err(IntegratedError::Runtime(ActionError::UndeliveredCommand))
    ));
}
#[test]
fn invalid_time_is_atomic_and_large_jumps_monitor_each_tick() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 0.0, 0.0);
    start(&mut r);
    assert!(r.advance_to(Duration::from_millis(101)).is_err());
    assert_eq!(r.runtime().now(), Duration::ZERO);
    r.advance_to(Duration::from_secs(1)).unwrap();
    assert_eq!(
        r.audit()
            .iter()
            .filter(|a| a.phase == "persistent-monitor")
            .count(),
        10
    );
}
#[test]
fn release_rechecks_permission_after_enqueue() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 0.0, 0.0);
    let token = r.request_lease("a", ActionPhase::Start).unwrap();
    r.dispatch("a", ActionPhase::Start, &token).unwrap();
    r.update_permission(support::observation("start", Status::Invalid, 2, 100))
        .unwrap();
    assert!(matches!(
        r.take_command(),
        Err(IntegratedError::Runtime(ActionError::ContractUnavailable(
            _
        )))
    ));
    assert!(r.estimator().commands().is_empty());
}
#[test]
fn changed_model_blocks_release_and_no_result_is_accepted() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 0.0, 0.0);
    let token = r.request_lease("a", ActionPhase::Start).unwrap();
    let command = r.dispatch("a", ActionPhase::Start, &token).unwrap();
    r.observe(Observation {
        id: "bad".into(),
        source: "sensor".into(),
        variable: Variable::Pressure,
        value: Reading::Number(0.9),
        error: Some(0.0),
        generated_at: Duration::ZERO,
        received_at: Duration::ZERO,
        max_age: Duration::from_secs(1),
        version: 1,
        quality: Quality::Inconsistent,
    })
    .unwrap();
    assert!(matches!(r.take_command(), Err(IntegratedError::Blocked(_))));
    assert!(matches!(
        r.submit_result(support::report(
            &command,
            1,
            Duration::ZERO,
            Some(true),
            true,
            Some(Status::Valid)
        )),
        Err(IntegratedError::Runtime(ActionError::UndeliveredCommand))
    ));
    assert!(r.estimator().commands().is_empty());
}
#[test]
fn result_cannot_predate_physical_release() {
    let mut r = fixture(Action::FanStart, Action::FanStop, 0, false, 0.0, 0.0);
    let token = r.request_lease("a", ActionPhase::Start).unwrap();
    r.dispatch("a", ActionPhase::Start, &token).unwrap();
    r.advance_to(TICK).unwrap();
    let c = r.take_command().unwrap().unwrap();
    assert!(matches!(
        r.submit_result(support::report(
            &c.runtime_command.unwrap().command_id,
            1,
            Duration::ZERO,
            Some(true),
            true,
            Some(Status::Valid)
        )),
        Err(IntegratedError::Runtime(ActionError::InvalidResult))
    ));
}

#[test]
fn commit_needs_acknowledgement_permission_and_model_support() {
    let mut r = fixture_with_class(
        Action::FanStart,
        Action::FanStop,
        0,
        false,
        0.0,
        0.0,
        Interruptibility::Irreversible,
    );
    let c = start(&mut r);
    assert!(matches!(
        r.request_lease("a", ActionPhase::Commit),
        Err(IntegratedError::Runtime(ActionError::StartNotAcknowledged))
    ));
    r.submit_result(support::report(
        &c.runtime_command.unwrap().command_id,
        1,
        Duration::ZERO,
        Some(true),
        false,
        None,
    ))
    .unwrap();
    let token = r.request_lease("a", ActionPhase::Commit).unwrap();
    r.update_permission(support::observation("commit", Status::Invalid, 2, 100))
        .unwrap();
    assert!(r.dispatch("a", ActionPhase::Commit, &token).is_err());
    r.update_permission(support::observation("commit", Status::Valid, 3, 100))
        .unwrap();
    let token = r.request_lease("a", ActionPhase::Commit).unwrap();
    r.dispatch("a", ActionPhase::Commit, &token).unwrap();
    assert_eq!(r.runtime().instances()["a"].state, ActionState::Committed);
    assert_eq!(
        r.take_command()
            .unwrap()
            .unwrap()
            .runtime_command
            .unwrap()
            .kind,
        CommandKind::Commit
    );
}
