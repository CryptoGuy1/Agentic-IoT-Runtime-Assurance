use dcra::actions::*;
use dcra::evidence::AssuranceStatus;
use dcra::runtime::EvaluationMode;
use dcra::simulation::*;
use std::time::Duration;
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
fn experiment(
    scenario: &str,
    profile: &str,
    baseline: Baseline,
    ablation: Ablation,
    mode: EvaluationMode,
) -> Simulation {
    Simulation::with_experiment(
        SimulationConfig {
            scenario: scenario.into(),
            baseline,
            seed: 42,
        },
        mode,
        NetworkConfig::profile(profile).unwrap(),
        ablation,
    )
    .unwrap()
}
fn run(scenario: &str, profile: &str) -> Simulation {
    let mut s = experiment(
        scenario,
        profile,
        Baseline::B5,
        Ablation::None,
        EvaluationMode::Incremental,
    );
    s.command("run 10s").unwrap();
    s
}
#[test]
fn network_delay_preserves_source_time_version_and_expiry() {
    let mut s = experiment(
        "S1",
        "delay",
        Baseline::B5,
        Ablation::None,
        EvaluationMode::Incremental,
    );
    s.command("advance 40ms").unwrap();
    let a = &s.runtime().assurance().evidence()["s1"];
    assert_eq!(a.observed_at, Duration::ZERO);
    assert_eq!(a.expires_at, ms(2000));
    assert_eq!(a.version, 1);
    assert_eq!(s.now(), ms(40));
}
#[test]
fn duplicate_packets_do_not_repeat_physical_effects_or_overwrite_results() {
    let s = run("S1", "duplicate");
    assert!(
        s.runtime()
            .instances()
            .values()
            .all(|i| i.state == ActionState::Completed)
    );
    assert!(s.events.iter().any(|e| e.kind == "ObservationRejected"));
    assert!(s.events.iter().any(|e| e.kind == "DuplicateCommandIgnored"));
    assert!(s.events.iter().any(|e| e.kind == "ResultRejected"));
    assert_eq!(s.metrics.releases, 4);
}
#[test]
fn reorder_rejects_older_versions_without_crashing() {
    let s = run("S1", "reorder");
    assert!(s.events.iter().any(|e| e.kind == "ObservationRejected"));
    let mut last = std::collections::BTreeMap::new();
    for e in s.runtime().assurance().audit() {
        if let dcra::runtime::AuditEvent::EvidenceUpdated { current, .. } = &e.event
            && let Some(v) = last.insert(current.evidence_id.clone(), current.version)
        {
            assert!(current.version > v);
        }
    }
}
#[test]
fn complete_loss_leaves_evidence_unknown_and_actions_unstarted() {
    let network = NetworkConfig {
        loss_per_mille: 1000,
        ..NetworkConfig::default()
    };
    let mut s = Simulation::with_experiment(
        SimulationConfig::default(),
        EvaluationMode::Incremental,
        network,
        Ablation::None,
    )
    .unwrap();
    s.command("run 5s").unwrap();
    assert!(!s.runtime().assurance().evidence().contains_key("s1"));
    assert_eq!(s.metrics.releases, 0);
    assert!(
        s.runtime()
            .instances()
            .values()
            .all(|i| i.started_at.is_none())
    );
}
#[test]
fn partition_drops_cloud_authority_but_local_fan_finishes() {
    let mut s = experiment(
        "S1",
        "partition",
        Baseline::B5,
        Ablation::None,
        EvaluationMode::Incremental,
    );
    s.command("run 3200ms").unwrap();
    let fan = s
        .runtime()
        .instances()
        .values()
        .find(|i| i.action_id == "fan")
        .unwrap();
    assert_eq!(fan.state, ActionState::Completed);
    assert_eq!(
        s.estimate().status("agent_available", s.now()),
        AssuranceStatus::Unknown
    );
    let door = s
        .runtime()
        .instances()
        .values()
        .find(|i| i.action_id == "door")
        .unwrap();
    assert!(door.started_at.is_none());
    s.command("run 5s").unwrap();
    assert!(s.ground_truth().door_closed);
}
#[test]
fn late_expired_observation_is_not_rejuvenated_at_receipt() {
    let n = NetworkConfig {
        delay_ms: 2500,
        ..NetworkConfig::default()
    };
    let mut s = Simulation::with_experiment(
        SimulationConfig::default(),
        EvaluationMode::Incremental,
        n,
        Ablation::None,
    )
    .unwrap();
    s.command("run 2500ms").unwrap();
    let a = &s.runtime().assurance().evidence()["s1"];
    assert_eq!(a.observed_at, Duration::ZERO);
    assert_eq!(a.expires_at, ms(2000));
    assert_eq!(a.value_at(s.now()).status(), AssuranceStatus::Unknown);
    assert_eq!(s.metrics.releases, 0);
}
#[test]
fn stale_leases_crossing_network_never_enqueue_actions() {
    let s = run("S1", "stale-lease");
    assert_eq!(s.metrics.releases, 0);
    assert!(s.runtime().commands().is_empty());
    assert!(s.events.iter().any(|e| e.kind == "AuthorityRejected"));
}
#[test]
fn selected_evidence_refresh_between_request_and_dispatch_rejects_lease() {
    let n = NetworkConfig {
        authority_delay_ms: 80,
        ..NetworkConfig::default()
    };
    let mut s = Simulation::with_experiment(
        SimulationConfig::default(),
        EvaluationMode::Incremental,
        n,
        Ablation::None,
    )
    .unwrap();
    s.command("advance 20ms").unwrap();
    s.command("sensor s1 valid 2s").unwrap();
    s.command("advance 60ms").unwrap();
    assert_eq!(s.metrics.releases, 0);
    assert!(s.events.iter().any(|e| e.kind == "AuthorityRejected"));
    s.command("advance 80ms").unwrap();
    assert_eq!(s.metrics.releases, 1);
}
#[test]
fn old_plan_authority_in_flight_cannot_start_cancelled_instance() {
    let n = NetworkConfig {
        authority_delay_ms: 80,
        ..NetworkConfig::default()
    };
    let mut s = Simulation::with_experiment(
        SimulationConfig::default(),
        EvaluationMode::Incremental,
        n,
        Ablation::None,
    )
    .unwrap();
    s.command("advance 0ms").unwrap();
    s.command("replan").unwrap();
    s.command("run 1s").unwrap();
    assert_eq!(
        s.runtime().instances()["v1.0.alarm"].state,
        ActionState::Cancelled
    );
    assert!(
        !s.runtime()
            .commands()
            .values()
            .any(|c| c.instance_id == "v1.0.alarm")
    );
    assert!(s.events.iter().any(|e| e.kind == "AuthorityRejected"));
}
#[test]
fn network_controller_delay_reports_a_missed_reaction_deadline() {
    let s = run("C13", "slow-controller");
    assert!(s.metrics.reactions[0].missed);
    assert!(
        s.events
            .iter()
            .any(|e| e.kind == "ReactionDeadlineMiss" && e.at == ms(1000))
    );
}
#[test]
fn network_randomness_is_keyed_and_not_shifted_by_other_traffic() {
    let mut a = Network::new(NetworkConfig::profile("compound").unwrap(), 42).unwrap();
    let mut b = a.clone();
    for i in 0..50 {
        let key = format!("s1:{i}");
        let expected = a.send(Link::Sensor, &key, ms(i * 500));
        b.send(Link::Result, &format!("unrelated:{i}"), ms(i * 500));
        assert_eq!(expected, b.send(Link::Sensor, &key, ms(i * 500)));
    }
}
#[test]
fn all_network_profiles_are_equivalent_in_full_and_incremental_modes() {
    for profile in [
        "none",
        "delay",
        "loss",
        "duplicate",
        "reorder",
        "partition",
        "compound",
        "stale-lease",
        "slow-controller",
    ] {
        let mut a = experiment(
            "S1",
            profile,
            Baseline::B5,
            Ablation::None,
            EvaluationMode::Full,
        );
        let mut b = experiment(
            "S1",
            profile,
            Baseline::B5,
            Ablation::None,
            EvaluationMode::Incremental,
        );
        a.command("run 10s").unwrap();
        b.command("run 10s").unwrap();
        assert_eq!(a.semantic_snapshot(), b.semantic_snapshot(), "{profile}");
    }
}
#[test]
fn feature_removals_produce_expected_counterexamples() {
    for (scenario, ablation) in [
        ("S2", Ablation::NoAlternatives),
        ("C13", Ablation::NoRun),
        ("S3", Ablation::NoCommit),
        ("C11", Ablation::NoPhysical),
    ] {
        let mut s = experiment(
            scenario,
            "none",
            Baseline::B5,
            ablation,
            EvaluationMode::Incremental,
        );
        if ablation == Ablation::NoAlternatives {
            s.command("fault s1 drop").unwrap();
        }
        s.command("run 10s").unwrap();
        match ablation {
            Ablation::NoAlternatives => assert_eq!(s.metrics.releases, 0),
            Ablation::NoRun => assert!(
                s.runtime()
                    .instances()
                    .values()
                    .all(|i| i.state == ActionState::Completed)
            ),
            Ablation::NoCommit => assert!(s.ground_truth().suppression_discharged),
            Ablation::NoPhysical => {
                assert!(s.ground_truth().fan_high && s.ground_truth().damper_closed)
            }
            _ => unreachable!(),
        }
    }
}
#[test]
fn compound_session_and_manifest_reproduce_semantic_history() {
    let mut s = experiment(
        "S5",
        "compound",
        Baseline::B5,
        Ablation::None,
        EvaluationMode::Incremental,
    );
    s.command("run 10s").unwrap();
    let r = Simulation::replay(&s.replay_text()).unwrap();
    assert_eq!(s.semantic_snapshot(), r.semantic_snapshot());
    let dir = std::env::temp_dir().join(format!("dcra-week-five-manifest-{}", std::process::id()));
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    s.save(&dir).unwrap();
    let r =
        Simulation::replay_manifest(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(s.semantic_snapshot(), r.semantic_snapshot());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn baseline_exogenous_sensor_fault_schedules_match() {
    let mut expected = None;
    for b in [
        Baseline::B0,
        Baseline::B1,
        Baseline::B2,
        Baseline::B3,
        Baseline::B4,
        Baseline::B5,
    ] {
        let mut s = experiment(
            "S5",
            "compound",
            b,
            Ablation::None,
            EvaluationMode::Incremental,
        );
        s.command("run 5s").unwrap();
        let transcript: Vec<_> = s
            .network
            .transmissions
            .iter()
            .filter(|t| {
                t.key.starts_with("sensor:s2:") || t.key.starts_with("sensor:occupancy_clear:")
            })
            .cloned()
            .collect();
        if let Some(ref e) = expected {
            assert_eq!(e, &transcript);
        } else {
            expected = Some(transcript);
        }
    }
}
#[test]
fn invalid_network_configuration_is_rejected_before_execution() {
    let n = NetworkConfig {
        loss_per_mille: 1001,
        ..NetworkConfig::default()
    };
    assert!(Network::new(n, 42).is_err());
    let n = NetworkConfig {
        partition: Some((500, 400)),
        ..NetworkConfig::default()
    };
    assert!(Network::new(n, 42).is_err());
}

#[test]
fn runtime_cancellation_does_not_teleport_to_the_actuator() {
    let mut s = experiment(
        "C13",
        "slow-controller",
        Baseline::B5,
        Ablation::None,
        EvaluationMode::Incremental,
    );
    s.command("run 650ms").unwrap();
    assert_eq!(
        s.runtime().instances().values().next().unwrap().state,
        ActionState::Aborting
    );
    // The older start reaches the actuator before the delayed abort does.
    assert!(s.ground_truth().fan_starting);
    s.command("run 800ms").unwrap();
    assert!(!s.ground_truth().fan_starting);
    assert!(s.metrics.reactions[0].missed);
}
