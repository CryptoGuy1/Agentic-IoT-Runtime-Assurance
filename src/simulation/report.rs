use super::*;
use crate::actions::ActionState;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
fn quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if c < ' ' => {
                write!(o, "\\u{:04x}", c as u32).unwrap();
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
fn hash(s: &str) -> String {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{h:016x}")
}
fn command(name: &str, args: &[&str]) -> String {
    Command::new(name)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unavailable".into())
}
fn rate(n: u64, d: u64) -> String {
    format!(
        "{{\"numerator\":{n},\"denominator\":{d},\"value\":{}}}",
        if d == 0 {
            "null".into()
        } else {
            format!("{:.6}", n as f64 / d as f64)
        }
    )
}
fn timing(values: &[u128]) -> String {
    format!(
        "{{\"samples\":{},\"mean_ns\":{},\"max_ns\":{}}}",
        values.len(),
        if values.is_empty() {
            "null".into()
        } else {
            (values.iter().sum::<u128>() / values.len() as u128).to_string()
        },
        values.iter().max().map_or("null".into(), |n| n.to_string())
    )
}
fn fault_schedule(scenario: &str) -> String {
    let faults: Vec<(u64, &str)> = match scenario {
        "S2" => vec![(
            500,
            "s1 observation becomes UNKNOWN; subsequent samples dropped",
        )],
        "S3" => vec![
            (
                0,
                "occupancy sample TTL=1050ms; no further occupancy samples",
            ),
            (1040, "zone 2 becomes occupied"),
            (1050, "occupancy evidence expires"),
        ],
        "S4" => vec![(500, "agent partition; agent availability UNKNOWN")],
        "S5" => vec![
            (500, "s1 dropout and seeded 1-in-5 packet loss"),
            (700, "human approval unavailable"),
            (1100, "zone 2 becomes occupied"),
        ],
        "C13" => vec![(
            500,
            "pressure support lost; detect=50ms propagate=20ms schedule=10ms actuate=100ms; margin=500ms",
        )],
        "C14" => vec![(
            500,
            "pressure support lost; detect=50ms propagate=20ms schedule=10ms actuate=650ms; margin=500ms",
        )],
        "C15" => vec![(600, "agent partition after door commit")],
        "C16" => vec![(800, "replace pending suffix")],
        _ => vec![],
    };
    format!(
        "[{}]",
        faults
            .iter()
            .map(|(t, d)| format!("{{\"time_ms\":{t},\"description\":{}}}", quote(d)))
            .collect::<Vec<_>>()
            .join(",")
    )
}
impl Simulation {
    pub fn metrics_json(&self) -> String {
        let m = &self.metrics;
        let current = &self.runtime().plans()["building"].actions;
        let completed = current
            .keys()
            .filter(|id| self.runtime().instances()[*id].state == ActionState::Completed)
            .count() as u64;
        let reactions = m.reactions.iter().map(|r| {
            let delta = |a: Option<std::time::Duration>, b: Option<std::time::Duration>| {
                a.zip(b).map_or("null".into(), |(a,b)| (a-b).as_nanos().to_string())
            };
            format!("{{\"instance\":{},\"fault_ns\":{},\"L_detect_ns\":{},\"L_propagate_ns\":{},\"L_schedule_ns\":{},\"L_actuate_ns\":{},\"total_ns\":{},\"margin_ns\":{},\"succeeded\":{},\"missed\":{}}}",
                quote(&r.instance_id), r.fault_at.as_nanos(), delta(r.detected_at,Some(r.fault_at)),
                delta(r.propagated_at,r.detected_at), delta(r.scheduled_at,r.propagated_at),
                delta(r.actuated_at,r.scheduled_at), delta(r.actuated_at,Some(r.fault_at)),
                r.margin.as_nanos(), r.succeeded.map_or("null".into(),|v|v.to_string()), r.missed)
        }).collect::<Vec<_>>().join(",");
        format!(
            "{{\n\"UnsafeActionReleaseRate\":{},\n\"FalseSafeAuthorizationRate\":{},\n\"UsefulActionRetention\":{},\n\"UnnecessaryRevocationRate\":{},\n\"WitnessSwitchCount\":{},\n\"LeaseRenewalCount\":{},\n\"ReplanCount\":{},\n\"FallbackCount\":{},\n\"DependencyPropagationLatency\":{},\n\"ExecutionRevalidationLatency\":{},\n\"LeaseIssuanceLatency\":{},\n\"SafetyReactionLatency\":[{}],\n\"ReactionDeadlineMisses\":{},\n\"TaskCompletionRate\":{},\n\"ActionCompletionRate\":{}\n}}\n",
            rate(m.unsafe_releases, m.releases),
            rate(m.false_safe_releases, m.releases),
            rate(
                m.retained_opportunities.len() as u64,
                m.useful_opportunities.len() as u64
            ),
            rate(m.unnecessary_revocations, m.revocations),
            m.witness_switches,
            m.lease_renewals,
            m.replans,
            m.fallbacks,
            timing(&self.work.iter().map(|w| w.elapsed_ns).collect::<Vec<_>>()),
            timing(&self.revalidation_ns),
            timing(&self.lease_issuance_ns),
            reactions,
            m.reactions.iter().filter(|r| r.missed).count(),
            rate(
                u64::from(!current.is_empty() && completed == current.len() as u64),
                1
            ),
            rate(completed, current.len() as u64)
        )
    }
    pub fn replay_text(&self) -> String {
        format!(
            "dcra-session-v2\nscenario {}\nbaseline {:?}\nseed {}\nnetwork {}\nablation {}\nmode {:?}\n{}{}",
            self.config.scenario,
            self.config.baseline,
            self.config.seed,
            self.network.config.encode(),
            self.ablation.name(),
            self.runtime().assurance().mode(),
            self.journal.join("\n"),
            if self.journal.is_empty() { "" } else { "\n" }
        )
    }
    pub fn replay(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let format = lines.next().ok_or("missing replay format")?;
        if !matches!(format, "dcra-session-v1" | "dcra-session-v2") {
            return Err("unsupported replay format".into());
        }
        let scenario = lines
            .next()
            .and_then(|s| s.strip_prefix("scenario "))
            .ok_or("missing scenario")?;
        let baseline = lines
            .next()
            .and_then(|s| s.strip_prefix("baseline "))
            .ok_or("missing baseline")?
            .parse()?;
        let seed = lines
            .next()
            .and_then(|s| s.strip_prefix("seed "))
            .ok_or("missing seed")?
            .parse()
            .map_err(|_| "invalid seed")?;
        let (network, ablation, mode) = if format == "dcra-session-v2" {
            let network = NetworkConfig::decode(
                lines
                    .next()
                    .and_then(|s| s.strip_prefix("network "))
                    .ok_or("missing network")?,
            )?;
            let ablation = lines
                .next()
                .and_then(|s| s.strip_prefix("ablation "))
                .ok_or("missing ablation")?
                .parse()?;
            let mode = match lines.next().and_then(|s| s.strip_prefix("mode ")) {
                Some("Full") => crate::runtime::EvaluationMode::Full,
                Some("Incremental") => crate::runtime::EvaluationMode::Incremental,
                _ => return Err("invalid evaluation mode".into()),
            };
            (network, ablation, mode)
        } else {
            (
                NetworkConfig::default(),
                Ablation::None,
                crate::runtime::EvaluationMode::Incremental,
            )
        };
        let mut s = Self::with_experiment(
            SimulationConfig {
                scenario: scenario.into(),
                baseline,
                seed,
            },
            mode,
            network,
            ablation,
        )?;
        for line in lines {
            s.command(line)?;
        }
        Ok(s)
    }
    /// Reads the escaped session string written in this program's own manifest.
    pub fn replay_manifest(text: &str) -> Result<Self, String> {
        let key = "\"replay_session\":";
        let tail = text
            .split_once(key)
            .ok_or("manifest has no replay_session; use session.commands for Week 4 reports")?
            .1
            .trim_start();
        let mut chars = tail.chars();
        if chars.next() != Some('"') {
            return Err("invalid manifest session".into());
        }
        let mut session = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Self::replay(&session),
                '\\' => match chars.next().ok_or("incomplete manifest escape")? {
                    'n' => session.push('\n'),
                    'r' => session.push('\r'),
                    't' => session.push('\t'),
                    '"' => session.push('"'),
                    '\\' => session.push('\\'),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        let cp =
                            u32::from_str_radix(&hex, 16).map_err(|_| "invalid unicode escape")?;
                        session.push(char::from_u32(cp).ok_or("invalid unicode scalar")?);
                    }
                    _ => return Err("unsupported manifest escape".into()),
                },
                c => session.push(c),
            }
        }
        Err("unterminated manifest session".into())
    }
    pub fn semantic_snapshot(&self) -> String {
        format!(
            "config={:?}\nnetwork={:?}\nablation={:?}\nclock={:?}\ntruth={:?}\ninstances={:?}\nleases={:?}\ncommands={:?}\ndecisions={:?}\nevents={:?}\nmetrics={:?}\n",
            self.config,
            self.network.config,
            self.ablation,
            self.now(),
            self.ground_truth(),
            self.runtime().instances(),
            self.runtime().leases(),
            self.runtime().commands(),
            self.runtime().decisions(),
            self.events,
            self.metrics
        )
    }
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        std::fs::create_dir(path).map_err(|e| format!("create new output directory: {e}"))?;
        let write =
            |name: &str, s: String| std::fs::write(path.join(name), s).map_err(|e| e.to_string());
        let graph = format!("{:?}", self.runtime().assurance().graph());
        let config = format!(
            "{:?}:{:?}:{:?}",
            self.config, self.network.config, self.ablation
        );
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let manifest = format!(
            "{{\"schema\":2,\"experiment_id\":{},\"git_commit\":{},\"git_status\":{},\"timestamp_unix_seconds\":{timestamp},\"language_version\":{},\"config_hash\":{},\"scenario\":{},\"seed\":{},\"runtime\":{},\"evaluation_mode\":{},\"plan_version\":{},\"graph_hash\":{},\"fault_schedule\":{},\"journal_hash\":{},\"model\":\"discrete-building-v1\",\"source_hash\":{},\"parameters\":{{\"sample_period_ms\":500,\"default_sensor_ttl_ms\":2000,\"ack_delay_ms\":10,\"controller_schedule_ms\":10,\"controller_actuate_ms\":100,\"pressure_minimum\":50}},\"network\":{},\"ablation\":{},\"replay_session\":{}}}\n",
            quote(&hash(&format!("{config}{}", self.replay_text()))),
            quote(&command("git", &["rev-parse", "HEAD"])),
            quote(&command("git", &["status", "--porcelain"])),
            quote(&command("rustc", &["--version"])),
            quote(&hash(&config)),
            quote(&self.config.scenario),
            self.config.seed,
            quote(&format!("{:?}", self.config.baseline)),
            quote(&format!("{:?}", self.runtime().assurance().mode())),
            self.runtime().plans()["building"].version,
            quote(&hash(&graph)),
            fault_schedule(&self.config.scenario),
            quote(&hash(&self.replay_text())),
            quote(&hash(concat!(
                include_str!("model.rs"),
                include_str!("config.rs"),
                include_str!("engine.rs"),
                include_str!("network.rs"),
                include_str!("report.rs"),
                include_str!("../actions/execution.rs"),
                include_str!("../actions/types.rs"),
                include_str!("../runtime.rs"),
                include_str!("../evaluator.rs"),
                include_str!("../incremental.rs"),
                include_str!("../graph.rs"),
                include_str!("../evidence.rs"),
                include_str!("../clock.rs"),
                include_str!("../../Cargo.toml")
            ))),
            quote(&self.network.config.encode()),
            quote(self.ablation.name()),
            quote(&self.replay_text())
        );
        let mut network_csv = "link,key,sent_ns,arrival_ns,copy,dropped\n".to_string();
        for t in &self.network.transmissions {
            if t.arrivals.is_empty() {
                writeln!(
                    network_csv,
                    "{:?},{},{},,,{}",
                    t.link,
                    t.key,
                    t.sent_at.as_nanos(),
                    t.dropped.as_deref().unwrap_or("")
                )
                .unwrap();
            }
            for (copy, at) in t.arrivals.iter().enumerate() {
                writeln!(
                    network_csv,
                    "{:?},{},{},{},{},",
                    t.link,
                    t.key,
                    t.sent_at.as_nanos(),
                    at.as_nanos(),
                    copy
                )
                .unwrap();
            }
        }
        write("network.csv", network_csv)?;
        write("manifest.json", manifest)?;
        write("metrics.json", self.metrics_json())?;
        let count = |kind: &str| self.events.iter().filter(|e| e.kind == kind).count();
        write(
            "network-metrics.json",
            format!(
                "{{\"transmissions\":{},\"scheduled_copies\":{},\"dropped_at_send\":{},\"dropped_at_receipt\":{},\"stale_observations\":{},\"authority_rejections\":{},\"duplicate_commands\":{},\"obsolete_commands\":{},\"rejected_results\":{}}}\n",
                self.network.transmissions.len(),
                self.network
                    .transmissions
                    .iter()
                    .map(|t| t.arrivals.len())
                    .sum::<usize>(),
                self.network
                    .transmissions
                    .iter()
                    .filter(|t| t.dropped.is_some())
                    .count(),
                count("NetworkDrop"),
                count("ObservationRejected"),
                count("AuthorityRejected"),
                count("DuplicateCommandIgnored"),
                count("ObsoleteCommandIgnored"),
                count("ResultRejected")
            ),
        )?;
        write("session.commands", self.replay_text())?;
        write("semantic.txt", self.semantic_snapshot())?;
        write(
            "events.jsonl",
            self.events
                .iter()
                .map(|e| {
                    format!(
                        "{{\"sequence\":{},\"time_ns\":{},\"event_type\":{},\"detail\":{}}}\n",
                        e.sequence,
                        e.at.as_nanos(),
                        quote(&e.kind),
                        quote(&e.detail)
                    )
                })
                .collect(),
        )?;
        let mut csv =
            "time_ns,evidence,N,M,M_e,nodes_recomputed,justifications_recomputed,propagation_ns\n"
                .to_string();
        for w in &self.work {
            writeln!(
                csv,
                "{},{},{},{},{},{},{},{}",
                w.at.as_nanos(),
                w.evidence,
                w.nodes,
                w.rules,
                w.affected_rules,
                w.nodes_recomputed,
                w.rules_recomputed,
                w.elapsed_ns
            )
            .unwrap();
        }
        write("workload.csv", csv)?;
        Ok(())
    }
}
