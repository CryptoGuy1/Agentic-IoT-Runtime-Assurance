use super::{
    benchmark::{TruePlant, TrueState, test_score},
    estimate::predict,
    interval::p,
    *,
};
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
fn prior() -> StateBox {
    StateBox {
        at: Duration::ZERO,
        hazard: Interval::new(0.65, 0.75).unwrap(),
        pressure: Interval::new(-0.05, 0.05).unwrap(),
        fan: p(0.0),
        door: p(0.0),
        occupied: BoolSet::False,
        alarm: BoolSet::False,
        fan_failed: BoolSet::False,
        door_jammed: BoolSet::False,
        response_required: BoolSet::True,
    }
}
fn exact_assumptions() -> Assumptions {
    Assumptions {
        disturbance_scale: 0.0,
        source: p(0.08),
        faults_may_start: false,
        occupancy_stable_until: ms(20000),
        ..Assumptions::default()
    }
}
fn obs(id: &str, variable: Variable, value: Reading, generated: u64, received: u64) -> Observation {
    Observation {
        id: id.into(),
        source: format!("sensor-{variable:?}"),
        variable,
        value,
        error: variable.error(),
        generated_at: ms(generated),
        received_at: ms(received),
        max_age: variable.max_age(),
        version: generated / 100 + 1,
        quality: Quality::Trusted,
    }
}
fn command(id: u64, action: Action, issued: u64, delay: u64, may_drop: bool) -> Command {
    Command {
        id,
        action,
        issued_at: ms(issued),
        max_delay: ms(delay),
        may_drop,
    }
}

#[test]
fn interval_arithmetic_encloses_endpoints_and_rejects_bad_numbers() {
    assert!(Interval::new(f64::NAN, 1.0).is_err());
    assert!(Interval::new(2.0, 1.0).is_err());
    assert!(Interval::point(f64::INFINITY).is_err());
    let a = Interval::new(-0.7, 0.3).unwrap();
    let b = Interval::new(-0.2, 0.8).unwrap();
    for x in [-0.7, -0.4, 0.0, 0.3] {
        for y in [-0.2, 0.0, 0.4, 0.8] {
            assert!(a.mul(b).contains(x * y));
            assert!(a.add(b).contains(x + y));
            assert!(a.sub(b).contains(x - y));
        }
    }
    assert!(p(0.1).add(p(0.2)).lo() < 0.1 + 0.2);
    assert!(p(0.1).add(p(0.2)).hi() > 0.1 + 0.2);
    assert_eq!(a.intersect(Interval::new(0.4, 0.5).unwrap()), None);
}

#[test]
fn scalar_plant_matches_hand_calculations_and_actual_outcomes() {
    let mut plant = TruePlant::new(TrueState::nominal(), Dynamics::default(), Inputs::default());
    plant.noise_scale = 0.0;
    plant.apply(&command(1, Action::FanStart, 0, 0, false));
    plant.step([0.0; 4]);
    assert_eq!(plant.state.fan, 0.1 / 1.5);
    assert!((plant.state.hazard - 0.7066).abs() < 1e-14);
    for _ in 1..40 {
        plant.step([0.0; 4]);
    }
    assert!(plant.state.fan >= 0.90);
    plant.apply(&command(2, Action::FanStop, 4000, 0, false));
    for _ in 0..60 {
        plant.step([0.0; 4]);
    }
    assert!(plant.state.fan <= 0.05);
    plant.apply(&command(3, Action::DoorClose, 10000, 0, false));
    for _ in 0..25 {
        plant.step([0.0; 4]);
    }
    assert!(plant.state.door >= 0.95);
    plant.apply(&command(4, Action::DoorOpen, 12500, 0, false));
    for _ in 0..25 {
        plant.step([0.0; 4]);
    }
    assert!(plant.state.door <= 0.05);
    plant.apply(&command(5, Action::AlarmOn, 15000, 0, false));
    plant.step([0.0; 4]);
    assert!(!plant.state.alarm);
    plant.step([0.0; 4]);
    assert!(plant.state.alarm);
}

#[test]
fn delayed_measurement_updates_history_without_resetting_current_fan() {
    let mut e = Estimator::new(prior(), Inputs::default(), exact_assumptions()).unwrap();
    e.note_command(command(1, Action::FanStart, 0, 0, false))
        .unwrap();
    e.advance_to(ms(400)).unwrap();
    let before = e.state().fan;
    assert_eq!(
        e.observe(obs("old", Variable::Fan, Reading::Number(0.0), 0, 400))
            .unwrap(),
        Update::Applied
    );
    assert!(!e.inconsistent());
    assert!(e.state().fan.lo() > 0.20);
    assert!(e.state().fan.subset_of(before));
}

#[test]
fn rejected_and_expired_observations_are_atomic_and_duplicates_do_not_reapply() {
    let mut e = Estimator::new(prior(), Inputs::default(), exact_assumptions()).unwrap();
    e.advance_to(ms(500)).unwrap();
    let before = e.state().clone();
    let future = obs("future", Variable::Pressure, Reading::Number(0.0), 600, 600);
    assert!(e.observe(future).is_err());
    assert_eq!(e.state(), &before);
    let mut bad = obs("bad", Variable::Fan, Reading::Number(0.0), 500, 500);
    bad.error = None;
    assert!(e.observe(bad).is_err());
    assert_eq!(e.state(), &before);
    let good = obs("good", Variable::Fan, Reading::Number(0.0), 500, 500);
    assert_eq!(e.observe(good.clone()).unwrap(), Update::Applied);
    let after = e.state().clone();
    assert_eq!(e.observe(good).unwrap(), Update::Duplicate);
    assert_eq!(e.state(), &after);
    assert_eq!(
        e.observe(obs(
            "reordered",
            Variable::Fan,
            Reading::Number(0.0),
            400,
            500
        ))
        .unwrap(),
        Update::Superseded
    );
    e.advance_to(ms(1500)).unwrap();
    let before = e.state().clone();
    assert_eq!(
        e.observe(obs(
            "expired",
            Variable::Pressure,
            Reading::Number(0.0),
            0,
            1500
        ))
        .unwrap(),
        Update::Expired
    );
    assert_eq!(e.state(), &before);
    assert!(e.advance_to(ms(1450)).is_err());
    assert_eq!(e.state(), &before);
}

#[test]
fn missing_quality_does_not_narrow_and_conflicts_latch_until_new_instance() {
    let mut e = Estimator::new(prior(), Inputs::default(), exact_assumptions()).unwrap();
    let before = e.state().clone();
    let mut missing = obs("missing", Variable::Pressure, Reading::Number(0.8), 0, 0);
    missing.quality = Quality::Missing;
    assert_eq!(e.observe(missing).unwrap(), Update::Missing);
    assert_eq!(e.state(), &before);
    let mut conflict = obs("conflict", Variable::Alarm, Reading::Boolean(true), 0, 0);
    conflict.source = "other".into();
    assert_eq!(e.observe(conflict).unwrap(), Update::Inconsistent);
    assert_eq!(e.check(Action::AlarmOn).decision, Decision::Unknown);
    e.advance_to(ms(500)).unwrap();
    e.observe(obs(
        "consistent",
        Variable::Alarm,
        Reading::Boolean(false),
        500,
        500,
    ))
    .unwrap();
    assert!(e.inconsistent());
    let reset = Estimator::new(prior(), Inputs::default(), exact_assumptions()).unwrap();
    assert!(!reset.inconsistent());
    let mut e = reset;
    let mut declared = obs("declared", Variable::Alarm, Reading::Boolean(false), 0, 0);
    declared.quality = Quality::Inconsistent;
    assert_eq!(e.observe(declared).unwrap(), Update::Inconsistent);
    assert_eq!(e.check(Action::FanStart).decision, Decision::Unknown);
}

#[test]
fn occupancy_can_change_before_ttl_and_protected_clearance_is_explicit() {
    let mut e = Estimator::new(prior(), Inputs::default(), Assumptions::default()).unwrap();
    e.observe(obs(
        "clear",
        Variable::Occupancy,
        Reading::Boolean(false),
        0,
        0,
    ))
    .unwrap();
    assert_eq!(e.state().occupied, BoolSet::False);
    assert_eq!(e.check(Action::DoorClose).decision, Decision::Unknown);
    e.advance_to(ms(100)).unwrap();
    assert_eq!(e.state().occupied, BoolSet::Either);
    let safe = Estimator::new(prior(), Inputs::default(), exact_assumptions()).unwrap();
    assert_eq!(safe.check(Action::DoorClose).decision, Decision::Supported);
    let mut short = exact_assumptions();
    short.occupancy_stable_until = ms(1000);
    assert_eq!(
        Estimator::new(prior(), Inputs::default(), short)
            .unwrap()
            .check(Action::DoorClose)
            .decision,
        Decision::Unknown
    );
}

#[test]
fn command_delay_loss_and_reordering_are_not_immediate_effects() {
    let mut e = Estimator::new(prior(), Inputs::default(), exact_assumptions()).unwrap();
    e.note_command(command(1, Action::FanStart, 0, 600, false))
        .unwrap();
    e.advance_to(ms(400)).unwrap();
    assert!(e.state().fan.contains(0.0));
    assert!(e.state().fan.hi() > 0.20);
    e.advance_to(ms(600)).unwrap();
    e.note_command(command(2, Action::FanStop, 600, 0, false))
        .unwrap();
    e.advance_to(ms(1000)).unwrap();
    assert!(e.state().fan.contains(0.0));
    let before = e.state().clone();
    assert!(
        e.note_command(command(1, Action::FanStart, 1000, 0, false))
            .is_err()
    );
    assert_eq!(e.state(), &before);
    let mut lost = Estimator::new(prior(), Inputs::default(), exact_assumptions()).unwrap();
    lost.note_command(command(1, Action::FanStart, 0, 600, true))
        .unwrap();
    lost.advance_to(ms(4000)).unwrap();
    assert!(lost.state().fan.contains(0.0));
    assert!(lost.state().fan.hi() > 0.90);
    let mut plant = TruePlant::new(TrueState::nominal(), Dynamics::default(), Inputs::default());
    assert!(plant.apply(&command(2, Action::FanStop, 0, 0, false)));
    assert!(!plant.apply(&command(1, Action::FanStart, 0, 0, false)));
    assert!(!plant.apply(&command(2, Action::FanStop, 0, 0, false)));
    plant.step([0.0; 4]);
    assert_eq!(plant.state.fan, 0.0);
}

#[test]
fn all_bounded_extreme_disturbances_and_hidden_modes_remain_contained() {
    for mask in 0..16 {
        let mut e = Estimator::new(prior(), Inputs::default(), Assumptions::default()).unwrap();
        let mut plant =
            TruePlant::new(TrueState::nominal(), Dynamics::default(), Inputs::default());
        let c = command(1, Action::FanStart, 0, 600, true);
        e.note_command(c.clone()).unwrap();
        for tick in 0..80 {
            if tick == 3 {
                plant.apply(&c);
            }
            if tick == 12 {
                plant.state.fan_failed = true;
                plant.state.door_jammed = true;
            }
            if tick == 8 {
                plant.state.occupied = true;
            }
            let noise = std::array::from_fn(|i| {
                [0.01, 0.02, 0.02, 0.01][i] * if mask & (1 << i) == 0 { -1.0 } else { 1.0 }
            });
            plant.step(noise);
            e.advance_to(plant.state.at).unwrap();
            assert!(
                plant.state.contained_by(e.state()),
                "mask={mask} tick={tick} truth={:?} estimate={:?}",
                plant.state,
                e.state()
            );
        }
    }
}

#[test]
fn widening_bounds_preserves_enclosure_and_supported_subset_property() {
    let s = prior();
    let mut narrow = exact_assumptions();
    narrow.disturbance_scale = 0.5;
    let mut wide = narrow.clone();
    wide.disturbance_scale = 2.0;
    let a = predict(&s, &narrow, Inputs::default(), &[]);
    let b = predict(&s, &wide, Inputs::default(), &[]);
    for (x, y) in [
        (a.hazard, b.hazard),
        (a.pressure, b.pressure),
        (a.fan, b.fan),
        (a.door, b.door),
    ] {
        assert!(x.subset_of(y));
    }
    let large = Estimator::new(s.clone(), Inputs::default(), wide.clone()).unwrap();
    let mut small = s;
    small.pressure = p(0.0);
    let small = Estimator::new(small, Inputs::default(), wide).unwrap();
    assert_eq!(large.check(Action::AlarmOn).decision, Decision::Supported);
    assert_eq!(small.check(Action::AlarmOn).decision, Decision::Supported);
}

#[test]
fn unsafe_initial_state_and_intermediate_path_cannot_be_hidden_by_endpoint() {
    let mut s = prior();
    s.pressure = p(0.9);
    let e = Estimator::new(s, Inputs::default(), exact_assumptions()).unwrap();
    assert_eq!(e.check(Action::AlarmOn).decision, Decision::Contradicted);
    let mut s = prior();
    s.pressure = Interval::new(0.79, 0.81).unwrap();
    let e = Estimator::new(s, Inputs::default(), exact_assumptions()).unwrap();
    assert_eq!(e.check(Action::AlarmOn).decision, Decision::Unknown);
    // Its terminal pressure has decayed below the threshold, but the initial uncertainty remains disqualifying.
    let mut end = e.state().clone();
    for _ in 0..20 {
        end = predict(&end, e.assumptions(), Inputs::default(), &[]);
    }
    assert!(end.pressure.hi() < 0.80);
}

#[test]
fn oracle_is_independent_and_safe_failure_is_separate_from_success() {
    let mut plant = TruePlant::new(TrueState::nominal(), Dynamics::default(), Inputs::default());
    plant.state.door_jammed = true;
    plant.noise_scale = 0.0;
    let before = plant.state.clone();
    let a = test_score(&plant, Action::DoorClose);
    assert!(a.physical_safe);
    assert!(!a.physical_success);
    assert!(!a.deadline_met);
    let mut wrong = prior();
    wrong.pressure = p(0.99);
    let gate = Estimator::new(wrong, Inputs::default(), exact_assumptions()).unwrap();
    assert_eq!(
        gate.check(Action::DoorClose).decision,
        Decision::Contradicted
    );
    assert_eq!(a, test_score(&plant, Action::DoorClose));
    assert_eq!(before, plant.state);
    plant.state.fan_failed = true;
    let a = test_score(&plant, Action::FanStart);
    assert!(!a.physical_success);
    assert!(a.physical_safe);
}

#[test]
fn component_fixtures_cover_truth_and_replay_exactly() {
    for name in SCENARIOS {
        let a = run_experiment(name, 42).unwrap();
        let b = run_experiment(name, 42).unwrap();
        assert_eq!(a.json, b.json, "{name}");
        assert_eq!(a.markdown, b.markdown, "{name}");
        if !["mismatch", "conflict"].contains(name) {
            assert_eq!(a.covered_ticks, a.total_ticks, "{name}");
            assert_eq!(a.inconsistent_ticks, 0, "{name}");
        }
    }
    let short = run_experiment("short-gap", 42).unwrap();
    assert_eq!(short.decision, Decision::Supported);
    assert!(short.json.contains("\"freshness_plus_model\":\"Unknown\""));
    assert_eq!(
        run_experiment("long-gap", 42).unwrap().decision,
        Decision::Unknown
    );
    assert_eq!(
        run_experiment("protected-room", 42).unwrap().decision,
        Decision::Supported
    );
    assert_eq!(
        run_experiment("occupancy-entry", 42).unwrap().decision,
        Decision::Unknown
    );
    let wrong = run_experiment("mismatch", 42).unwrap();
    assert!(wrong.covered_ticks < wrong.total_ticks);
    assert_eq!(
        run_experiment("conflict", 42).unwrap().decision,
        Decision::Unknown
    );
}

#[test]
fn no_observations_can_contract_uncertainty_without_losing_truth() {
    let mut s = prior();
    s.pressure = Interval::new(-1.0, 1.0).unwrap();
    let next = predict(&s, &Assumptions::default(), Inputs::default(), &[]);
    assert!(next.pressure.width() < s.pressure.width());
    assert!(next.pressure.contains(0.0));
}

#[test]
fn malformed_public_checker_inputs_return_unknown_without_panicking() {
    let bad = Command {
        id: 1,
        action: Action::FanStart,
        issued_at: Duration::MAX,
        max_delay: Duration::MAX,
        may_drop: false,
    };
    assert_eq!(
        check_action(
            &prior(),
            &Assumptions::default(),
            Inputs::default(),
            &[bad],
            &[],
            false,
            Action::FanStart
        )
        .decision,
        Decision::Unknown
    );
    let mut assumptions = Assumptions::default();
    assumptions.dynamics.fan_tau = f64::NAN;
    assert_eq!(
        check_action(
            &prior(),
            &assumptions,
            Inputs::default(),
            &[],
            &[],
            false,
            Action::FanStart
        )
        .decision,
        Decision::Unknown
    );
}

#[test]
fn coverage_with_delays_and_faults_is_not_specific_to_one_noise_seed() {
    for seed in 0..5 {
        for scenario in [
            "normal",
            "delayed-reading",
            "delayed-command",
            "lost-command",
            "fan-failure",
            "door-jam",
            "protected-room",
        ] {
            let r = run_experiment(scenario, seed).unwrap();
            assert_eq!(
                r.covered_ticks, r.total_ticks,
                "seed={seed} scenario={scenario}"
            );
            assert_eq!(r.inconsistent_ticks, 0, "seed={seed} scenario={scenario}");
        }
    }
}
