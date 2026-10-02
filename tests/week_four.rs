use dcra::actions::*;
use dcra::runtime::EvaluationMode;
use dcra::simulation::*;
use std::time::Duration;
fn sim(scenario: &str, baseline: Baseline) -> Simulation {
    Simulation::new(SimulationConfig {
        scenario: scenario.into(),
        baseline,
        seed: 42,
    })
    .unwrap()
}
fn run(scenario: &str, baseline: Baseline) -> Simulation {
    let mut s = sim(scenario, baseline);
    s.command("run 10s").unwrap();
    s
}
fn state(s: &Simulation, a: &str) -> ActionState {
    s.runtime()
        .instances()
        .values()
        .find(|i| i.action_id == a)
        .unwrap()
        .state
}
#[test]
fn s1_all_six_baselines_complete_nominal_plan() {
    for b in [
        Baseline::B0,
        Baseline::B1,
        Baseline::B2,
        Baseline::B3,
        Baseline::B4,
        Baseline::B5,
    ] {
        let s = run("S1", b);
        assert!(
            s.runtime()
                .instances()
                .values()
                .all(|i| i.state == ActionState::Completed),
            "{b:?}: {}",
            s.status()
        );
    }
}
#[test]
fn s2_redundant_sensor_loss_retains_b5_but_flat_baseline_stops() {
    let s = run("S2", Baseline::B5);
    assert_eq!(state(&s, "fan"), ActionState::Completed);
    let flat = run("S2", Baseline::B2);
    assert_eq!(state(&flat, "fan"), ActionState::Cancelled);
    assert!(s.metrics.witness_switches > 0);
}
#[test]
fn s3_expired_occupancy_blocks_commit_and_ground_truth_exposes_b0() {
    let s = run("S3", Baseline::B5);
    assert!(!s.ground_truth().suppression_discharged);
    assert_eq!(s.metrics.false_safe_releases, 0);
    assert!(
        !s.runtime()
            .commands()
            .values()
            .any(|c| c.kind == CommandKind::Commit)
    );
    let b = run("S3", Baseline::B0);
    assert!(b.ground_truth().suppression_discharged);
    assert!(b.metrics.unsafe_releases > 0);
    assert!(b.events.iter().any(|e| e.kind == "InvariantViolation"));
}
#[test]
fn s4_partition_preserves_local_running_obligations() {
    let s = run("S4", Baseline::B5);
    assert_eq!(state(&s, "fan"), ActionState::Completed);
    assert_eq!(state(&s, "door"), ActionState::Pending);
}
#[test]
fn s5_mixed_faults_retain_supported_work() {
    let s = run("S5", Baseline::B5);
    assert_eq!(state(&s, "alarm"), ActionState::Completed);
    assert_eq!(state(&s, "fan"), ActionState::Completed);
    assert_eq!(state(&s, "door"), ActionState::Pending);
    assert!(s.events.iter().any(|e| e.kind == "PacketDropped"));
}
#[test]
fn c11_joint_unsafe_actions_do_not_both_execute() {
    let s = run("C11", Baseline::B5);
    assert_eq!(state(&s, "damper"), ActionState::Completed);
    assert_eq!(state(&s, "fan"), ActionState::Pending);
    assert_eq!(s.metrics.unsafe_releases, 0);
    let b = run("C11", Baseline::B0);
    assert!(b.ground_truth().fan_high && b.ground_truth().damper_closed);
    assert!(b.metrics.unsafe_releases > 0);
}
#[test]
fn c12_safe_concurrency_has_equal_start_times() {
    let s = run("C12", Baseline::B5);
    let times: Vec<_> = s
        .runtime()
        .instances()
        .values()
        .map(|i| i.started_at)
        .collect();
    assert_eq!(times, vec![Some(Duration::ZERO); 2]);
    assert!(
        s.runtime()
            .instances()
            .values()
            .all(|i| i.state == ActionState::Completed)
    );
}
#[test]
fn c13_reaction_stages_fit_margin() {
    let s = run("C13", Baseline::B5);
    let r = &s.metrics.reactions[0];
    assert_eq!(
        r.actuated_at.unwrap() - r.fault_at,
        Duration::from_millis(180)
    );
    assert!(!r.missed);
    assert_eq!(state(&s, "fan"), ActionState::Cancelled);
}
#[test]
fn c14_deadline_miss_is_explicit() {
    let s = run("C14", Baseline::B5);
    assert!(s.metrics.reactions[0].missed);
    let miss = s
        .events
        .iter()
        .find(|e| e.kind == "ReactionDeadlineMiss")
        .unwrap();
    assert_eq!(miss.at, Duration::from_secs(1));
}
#[test]
fn c15_committed_door_completes_locally_after_partition() {
    let s = run("C15", Baseline::B5);
    assert_eq!(state(&s, "door"), ActionState::Completed);
    assert!(s.ground_truth().door_closed);
    assert!(
        s.runtime()
            .commands()
            .values()
            .any(|c| c.kind == CommandKind::CertifiedComplete)
    );
    assert!(!s.events.iter().any(|e| e.kind == "InvariantViolation"));
}
#[test]
fn c16_replanning_preserves_completed_and_committed_history() {
    let s = run("C16", Baseline::B5);
    assert_eq!(s.runtime().plans()["building"].version, 2);
    for id in ["v1.0.alarm", "v1.1.door"] {
        let i = &s.runtime().instances()[id];
        assert_eq!(i.plan_version, 1);
        assert_eq!(i.state, ActionState::Completed);
        assert_eq!(
            s.runtime()
                .commands()
                .values()
                .filter(|c| c.instance_id == id && c.kind == CommandKind::Start)
                .count(),
            1
        );
    }
    assert_eq!(
        s.runtime().instances()["v1.2.fan"].state,
        ActionState::Cancelled
    );
    assert_eq!(
        s.runtime().instances()["v2.2.fan"].state,
        ActionState::Completed
    );
}
#[test]
fn replay_reproduces_semantics_and_invalid_commands_do_not_mutate() {
    let mut s = sim("S1", Baseline::B5);
    for c in [
        "advance 0ms",
        "fault s1 drop",
        "advance 1s",
        "fault s1 restore",
        "ack fan drop",
        "run 7s",
    ] {
        s.command(c).unwrap();
    }
    let before = s.semantic_snapshot();
    assert!(s.command("sensor made-up valid").is_err());
    assert_eq!(before, s.semantic_snapshot());
    let replay = Simulation::replay(&s.replay_text()).unwrap();
    assert_eq!(s.semantic_snapshot(), replay.semantic_snapshot());
}
#[test]
fn full_and_incremental_lifecycle_scenarios_agree() {
    for scenario in [
        "S1", "S2", "S3", "S4", "S5", "C11", "C12", "C13", "C14", "C15", "C16",
    ] {
        let mut inc = sim(scenario, Baseline::B5);
        let mut full = Simulation::with_mode(inc.config.clone(), EvaluationMode::Full).unwrap();
        inc.command("run 10s").unwrap();
        full.command("run 10s").unwrap();
        assert_eq!(
            inc.semantic_snapshot(),
            full.semantic_snapshot(),
            "{scenario}"
        );
    }
}
#[test]
fn supported_continuation_matches_exhaustive_feasibility() {
    for mask in 0u32..(1 << ACTIONS.len()) {
        let candidates: Vec<_> = ACTIONS
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, a)| a.to_string())
            .collect();
        let selected = supported_subset(&candidates, &[]);
        assert!(selected.iter().all(|a| candidates.contains(a)));
        for (i, a) in selected.iter().enumerate() {
            assert!(selected[i + 1..].iter().all(|b| joint_safe(a, b)));
        }
        for a in candidates.iter().filter(|a| !selected.contains(a)) {
            assert!(selected.iter().any(|b| !joint_safe(a, b)));
        }
    }
}
#[test]
fn missing_acknowledgement_cannot_unlock_successor() {
    let mut s = sim("S1", Baseline::B5);
    s.command("ack alarm drop").unwrap();
    s.command("run 10s").unwrap();
    assert_ne!(state(&s, "alarm"), ActionState::Completed);
    assert!(
        s.runtime()
            .instances()
            .values()
            .find(|i| i.action_id == "fan")
            .unwrap()
            .started_at
            .is_none()
    );
}
#[test]
fn reports_are_written_without_overwriting_existing_results() {
    let s = run("S1", Baseline::B5);
    let path = std::env::temp_dir().join(format!("dcra-report-{}", std::process::id()));
    if path.exists() {
        std::fs::remove_dir_all(&path).unwrap();
    }
    s.save(&path).unwrap();
    for name in [
        "manifest.json",
        "metrics.json",
        "events.jsonl",
        "session.commands",
        "workload.csv",
        "semantic.txt",
    ] {
        assert!(path.join(name).is_file());
    }
    assert!(s.save(&path).is_err());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn invalid_suffix_replacement_is_atomic_and_cannot_reorder_executed_work() {
    use std::collections::{BTreeMap, BTreeSet};
    let (g, d) = configuration(Baseline::B5);
    let mut runtime = ActionRuntime::new(g, d).unwrap();
    runtime
        .register_plan(Plan {
            plan_id: "p".into(),
            version: 1,
            actions: BTreeMap::from([("old".into(), "alarm".into())]),
            precedence: BTreeSet::new(),
            created_at: Duration::ZERO,
        })
        .unwrap();
    let instances = runtime.instances().clone();
    let plans = runtime.plans().clone();
    let audit = runtime.audit().to_vec();
    assert!(
        runtime
            .replace_pending_suffix(
                "p",
                2,
                BTreeMap::from([("new".into(), "fan".into())]),
                BTreeSet::from([("new".into(), "new".into())])
            )
            .is_err()
    );
    assert_eq!(runtime.instances(), &instances);
    assert_eq!(runtime.plans(), &plans);
    assert_eq!(runtime.audit(), audit);
    runtime
        .replace_pending_suffix(
            "p",
            2,
            BTreeMap::from([("new".into(), "fan".into())]),
            BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(runtime.instances()["old"].state, ActionState::Cancelled);
    assert_eq!(runtime.instances()["new"].plan_version, 2);
}
#[test]
fn ground_truth_changes_are_not_silently_observed_by_the_runtime() {
    use dcra::evidence::AssuranceStatus;
    let mut s = sim("S1", Baseline::B5);
    s.command("advance 0ms").unwrap();
    let before = s.runtime().assurance().evaluation().clone();
    s.command("plant occupancy occupied").unwrap();
    assert!(s.ground_truth().occupied[1]);
    assert_eq!(s.runtime().assurance().evaluation(), &before);
    assert_eq!(
        s.estimate().status("occupancy_clear", s.now()),
        AssuranceStatus::Valid
    );
    s.command("advance 500ms").unwrap();
    assert_eq!(
        s.estimate().status("occupancy_clear", s.now()),
        AssuranceStatus::Invalid
    );
}
#[test]
fn workload_includes_internal_outcomes_and_expiry_recomputations() {
    let s = run("S3", Baseline::B5);
    assert!(s.work.iter().any(|w| w.evidence == "alarm.ack"));
    assert!(
        s.work
            .iter()
            .any(|w| w.evidence.contains("occupancy_clear") && w.at == Duration::from_millis(1050))
    );
    assert_eq!(s.work.len(), s.runtime().assurance().work_log().len());
    assert!(
        s.work
            .iter()
            .all(|w| w.affected_rules <= w.rules && w.nodes_recomputed <= w.nodes)
    );
}
#[test]
fn clock_jump_and_fine_steps_deliver_reactions_at_the_same_boundary() {
    let mut large = sim("C13", Baseline::B5);
    large.command("run 2s").unwrap();
    let mut small = sim("C13", Baseline::B5);
    for _ in 0..20 {
        small.command("advance 100ms").unwrap();
    }
    assert_eq!(large.semantic_snapshot(), small.semantic_snapshot());
}
#[test]
fn b4_is_one_shot_while_b5_monitors_running_support() {
    for baseline in [Baseline::B4, Baseline::B5] {
        let mut s = sim("S2", baseline);
        s.command("advance 100ms").unwrap();
        s.command("fault pressure_safe drop").unwrap();
        s.command("run 5s").unwrap();
        assert_eq!(
            state(&s, "fan"),
            if baseline == Baseline::B5 {
                ActionState::Cancelled
            } else {
                ActionState::Completed
            }
        );
    }
}
#[test]
fn all_baselines_run_the_same_five_scenario_definitions() {
    for scenario in ["S1", "S2", "S3", "S4", "S5"] {
        for baseline in [
            Baseline::B0,
            Baseline::B1,
            Baseline::B2,
            Baseline::B3,
            Baseline::B4,
            Baseline::B5,
        ] {
            let s = run(scenario, baseline);
            assert_eq!(s.now(), Duration::from_secs(10));
            assert!(
                s.events.windows(2).all(
                    |pair| pair[0].sequence + 1 == pair[1].sequence && pair[0].at <= pair[1].at
                )
            );
        }
    }
}

#[test]
fn suppression_preparation_is_not_misreported_as_irreversible_release() {
    let mut s = sim("S3", Baseline::B5);
    s.command("plant occupancy occupied").unwrap();
    s.command("run 1s").unwrap();
    assert_eq!(state(&s, "discharge"), ActionState::Executing);
    assert!(!s.ground_truth().suppression_discharged);
    assert_eq!(s.metrics.unsafe_releases, 0);
    assert_eq!(s.metrics.false_safe_releases, 0);
    s.command("run 1s").unwrap();
    assert!(
        !s.runtime()
            .commands()
            .values()
            .any(|c| c.kind == CommandKind::Commit)
    );
}
