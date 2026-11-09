use dcra::model_v1::*;

#[test]
fn fixed_building_scenarios_match_atomic_monitor_without_certificates() {
    for scenario in CLOSED_LOOP_SCENARIOS {
        let evidra = run_closed_loop(scenario, 42).unwrap();
        let monitor = run_closed_loop_with_options(
            scenario,
            42,
            RunOptions {
                atomic_monitor: true,
                ..RunOptions::default()
            },
        )
        .unwrap();
        assert_eq!(evidra.start_allowed, monitor.start_allowed, "{scenario}");
        assert_eq!(evidra.score, monitor.score, "{scenario}");
        assert_eq!(evidra.candidates, monitor.candidates, "{scenario}");
        assert_eq!(evidra.final_state, monitor.final_state, "{scenario}");
        assert!(!monitor.events_jsonl.contains("certificate-published"));
        assert!(evidra.events_jsonl.contains("certificate-published"));
        let releases = |r: &ClosedLoopReport| {
            r.events_jsonl
                .lines()
                .filter(|line| {
                    line.contains("physical-command-released")
                        || line.contains("command-release-blocked")
                })
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(releases(&evidra), releases(&monitor), "{scenario}");
        for report in [&evidra, &monitor] {
            let lines = report.events_jsonl.lines().collect::<Vec<_>>();
            let mut release_count = 0;
            for (i, line) in lines.iter().enumerate() {
                if line.contains("physical-command-released") {
                    release_count += 1;
                    // The harness asserts check.checked_at == runtime.now ==
                    // plant.now before emitting this adjacent trace record.
                    assert!(
                        i > 0 && lines[i - 1].contains("atomic-release-check"),
                        "{scenario}"
                    );
                }
            }
            assert_eq!(release_count, report.score.commands.len(), "{scenario}");
        }
        println!(
            "{scenario}: 0 release-decision disagreements; {} commands; identical execution score",
            monitor.score.commands.len()
        );
    }
}

#[test]
fn atomic_monitor_rejects_unmatched_policy() {
    assert!(
        run_closed_loop_with_options(
            "normal",
            42,
            RunOptions {
                atomic_monitor: true,
                policy: RefinementPolicy::B0,
                ..RunOptions::default()
            }
        )
        .unwrap_err()
        .contains("matched B6")
    );
}

#[test]
fn generated_parameter_families_execute_with_atomic_checks_and_recorded_order() {
    // Engineering fixtures outside the reserved development/held-out ranges;
    // these are not observations for the 300-draw validation gate.
    for seed in [u64::MAX, u64::MAX - 1, u64::MAX - 2] {
        for no_invalidation in [true, false] {
            let variant = BuildingVariant::draw(seed, no_invalidation);
            let mut options = RunOptions {
                variant: Some(variant),
                ..RunOptions::default()
            };
            let evidra = run_closed_loop_with_options("normal", 42, options).unwrap();
            options.atomic_monitor = true;
            let monitor = run_closed_loop_with_options("normal", 42, options).unwrap();
            assert_eq!(evidra.score, monitor.score);
            assert_eq!(evidra.candidates, monitor.candidates);
            assert_eq!(evidra.start_allowed, monitor.start_allowed);
            assert!(monitor.json.contains(BUILDING_GENERATOR_VERSION));
            let mut awaiting_delivery = false;
            for line in monitor.events_jsonl.lines() {
                if line.contains("runtime-advance-before-delivery-end") {
                    assert!(!awaiting_delivery);
                    awaiting_delivery = true;
                }
                if line.contains("sensor-delivery-stage-start") {
                    // Initial t=0 has no preceding advance; later stages do.
                    awaiting_delivery = false;
                }
                if line.contains("atomic-release-check") {
                    assert!(!awaiting_delivery);
                }
            }
            assert!(!awaiting_delivery);
        }
    }
}
