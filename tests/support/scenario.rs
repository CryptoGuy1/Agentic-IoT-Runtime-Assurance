use dcra::evidence::{AssuranceStatus as Status, EvidenceAtom};
use dcra::graph::{AssuranceGraph, Justification, Node, NodeKind};
use dcra::runtime::{AssuranceRuntime, EvaluationMode};
use std::collections::BTreeSet;
use std::fmt::Write;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Update(EvidenceAtom),
    Advance(Duration),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub seed: u64,
    pub nodes: Vec<Node>,
    pub rules: Vec<Justification>,
    pub operations: Vec<Operation>,
}

pub fn atom(id: &str, status: Status, version: u64, observed: u64, expiry: u64) -> EvidenceAtom {
    EvidenceAtom {
        evidence_id: id.into(),
        evidence_type: "sensor".into(),
        predicate: "high".into(),
        source_id: id.into(),
        version,
        observed_at: Duration::from_secs(observed),
        expires_at: Duration::from_secs(expiry),
        status,
        payload_hash: None,
    }
}
pub fn rule(id: &str, premises: &[&str], k: usize, conclusion: &str) -> Justification {
    Justification {
        justification_id: id.into(),
        premises: premises.iter().map(|s| (*s).into()).collect(),
        threshold: k,
        conclusion: conclusion.into(),
    }
}

// SplitMix64: fixed arithmetic and explicit wrapping make seeds portable.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

pub fn generate(seed: u64, size: usize, operations: usize) -> Scenario {
    let mut rng = Rng(seed);
    let leaves = (size / 2).max(1);
    let nodes: Vec<_> = (0..size)
        .map(|i| {
            Node::new(
                format!("n{i:03}"),
                if i < leaves {
                    NodeKind::Evidence
                } else if i + 1 == size {
                    NodeKind::ActionStart
                } else {
                    NodeKind::Derived
                },
            )
        })
        .collect();
    let mut rules = Vec::new();
    for i in leaves..size {
        for alternative in 0..(1 + rng.pick(3)) {
            let count = 1 + rng.pick(i.min(5));
            let mut selected = BTreeSet::new();
            while selected.len() < count {
                selected.insert(rng.pick(i));
            }
            rules.push(Justification {
                justification_id: format!("r{i:03}-{alternative}"),
                premises: selected
                    .into_iter()
                    .map(|j| nodes[j].node_id.clone())
                    .collect(),
                threshold: 1 + rng.pick(count),
                conclusion: nodes[i].node_id.clone(),
            });
        }
    }
    let mut scenario = Scenario {
        seed,
        nodes,
        rules,
        operations: Vec::new(),
    };
    let mut versions = vec![0; leaves];
    let mut now = 0;
    for _ in 0..operations {
        match rng.pick(10) {
            0..=6 => {
                let index = rng.pick(leaves);
                versions[index] += 1;
                let status = match rng.pick(5) {
                    0 => Status::Invalid,
                    1 => Status::Unknown,
                    _ => Status::Valid,
                };
                let expiry = now + rng.pick(6) as u64;
                scenario.operations.push(Operation::Update(atom(
                    &scenario.nodes[index].node_id,
                    status,
                    versions[index],
                    now,
                    expiry,
                )));
            }
            7 => {
                // Version zero may be a first observation; otherwise this is a stale update.
                let index = rng.pick(leaves);
                scenario.operations.push(Operation::Update(atom(
                    &scenario.nodes[index].node_id,
                    Status::Valid,
                    versions[index],
                    now,
                    now + 2,
                )));
            }
            8 => {
                let index = rng.pick(leaves);
                scenario.operations.push(Operation::Update(atom(
                    &scenario.nodes[index].node_id,
                    Status::Valid,
                    versions[index] + 1,
                    now + 1,
                    now + 3,
                )));
            }
            _ => {
                now += 1 + rng.pick(5) as u64;
                scenario
                    .operations
                    .push(Operation::Advance(Duration::from_secs(now)));
            }
        }
    }
    scenario
}

fn assert_same(a: &AssuranceRuntime, b: &AssuranceRuntime) {
    assert_eq!(a.now(), b.now());
    assert_eq!(a.epoch(), b.epoch());
    assert_eq!(a.evidence(), b.evidence());
    assert_eq!(a.evaluation(), b.evaluation());
    assert_eq!(a.audit(), b.audit());
}
fn oracle_check(runtime: &AssuranceRuntime) {
    if runtime.graph().nodes().len() <= 15 {
        super::oracle::verify(
            runtime.graph(),
            runtime.evidence(),
            runtime.now(),
            runtime.evaluation(),
        );
    }
}

pub fn run(scenario: &Scenario) {
    let mut operation_index = 0;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let graph = AssuranceGraph::new(scenario.nodes.clone(), scenario.rules.clone()).unwrap();
        let mut full = AssuranceRuntime::with_mode(graph.clone(), EvaluationMode::Full);
        let mut incremental = AssuranceRuntime::new(graph.clone());
        // These additional runtimes step through every intermediate expiry batch.
        let mut stepped_full = AssuranceRuntime::with_mode(graph.clone(), EvaluationMode::Full);
        let mut stepped_incremental = AssuranceRuntime::new(graph);
        assert_same(&full, &incremental);
        oracle_check(&full);
        for (index, operation) in scenario.operations.iter().enumerate() {
            operation_index = index;
            match operation {
                Operation::Update(atom) => {
                    let before = full.evaluation().clone();
                    let before_evidence = full.evidence().clone();
                    let before_audit = full.audit().to_vec();
                    let before_epoch = full.epoch();
                    let a = full.update(atom.clone());
                    assert_eq!(a, incremental.update(atom.clone()));
                    assert_eq!(a, stepped_full.update(atom.clone()));
                    assert_eq!(a, stepped_incremental.update(atom.clone()));
                    if a.is_err() {
                        assert_eq!(full.evidence(), &before_evidence);
                        assert_eq!(full.audit(), before_audit);
                        assert_eq!(full.epoch(), before_epoch);
                        assert_eq!(full.evaluation(), &before);
                    } else {
                        // Independently find the potential affected region by scanning rules.
                        let mut reachable = BTreeSet::from([atom.evidence_id.clone()]);
                        loop {
                            let count = reachable.len();
                            for rule in &scenario.rules {
                                if rule.premises.iter().any(|id| reachable.contains(id)) {
                                    reachable.insert(rule.conclusion.clone());
                                }
                            }
                            if reachable.len() == count {
                                break;
                            }
                        }
                        for id in full
                            .graph()
                            .nodes()
                            .keys()
                            .filter(|id| !reachable.contains(*id))
                        {
                            assert_eq!(
                                before.assurance_by_node[id],
                                full.evaluation().assurance_by_node[id],
                                "unrelated node {id}"
                            );
                            assert_eq!(
                                before.preferred_witness_by_node.get(id),
                                full.evaluation().preferred_witness_by_node.get(id),
                                "unrelated witness {id}"
                            );
                        }
                        assert!(
                            incremental.operation_stats().nodes_evaluated <= reachable.len(),
                            "evaluated outside affected region or repeated a node"
                        );
                    }
                }
                Operation::Advance(target) => {
                    let due: BTreeSet<_> = stepped_full
                        .evidence()
                        .values()
                        .filter(|a| {
                            a.status == Status::Valid
                                && a.expires_at <= *target
                                && a.expires_at >= stepped_full.now()
                        })
                        .map(|a| a.expires_at)
                        .collect();
                    for deadline in due {
                        assert_eq!(
                            stepped_full.advance_to(deadline),
                            stepped_incremental.advance_to(deadline)
                        );
                        assert_same(&stepped_full, &stepped_incremental);
                        oracle_check(&stepped_full);
                    }
                    assert_eq!(
                        stepped_full.advance_to(*target),
                        stepped_incremental.advance_to(*target)
                    );
                    assert_eq!(full.advance_to(*target), incremental.advance_to(*target));
                }
            }
            assert_same(&full, &incremental);
            assert_same(&stepped_full, &stepped_incremental);
            assert_same(&full, &stepped_full);
            oracle_check(&full);
        }
    }));
    if let Err(error) = outcome {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/week-two-failures");
        let path = dir.join(format!(
            "seed-{}-nodes-{}-op-{operation_index}.trace",
            scenario.seed,
            scenario.nodes.len()
        ));
        let saved =
            std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, scenario.encode()));
        eprintln!(
            "Week 2 failure: seed={}, operation={operation_index}, trace={}, save={saved:?}",
            scenario.seed,
            path.display()
        );
        std::panic::resume_unwind(error);
    }
}

// Versioned text format. Strings are hex-encoded UTF-8 so IDs and metadata can
// contain spaces, tabs or newlines; '_' denotes an empty string, '-' no payload.
fn hex(s: &str) -> String {
    if s.is_empty() {
        "_".into()
    } else {
        s.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }
}
fn unhex(s: &str) -> Result<String, String> {
    if s == "_" {
        return Ok(String::new());
    }
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err("invalid hex string".into());
    }
    let bytes = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}
fn time(d: Duration) -> String {
    format!("{}:{}", d.as_secs(), d.subsec_nanos())
}
fn parse_time(s: &str) -> Result<Duration, String> {
    let (secs, nanos) = s.split_once(':').ok_or("invalid time")?;
    let nanos: u32 = nanos.parse().map_err(|_| "invalid nanoseconds")?;
    if nanos >= 1_000_000_000 {
        return Err("nanoseconds out of range".into());
    }
    Ok(Duration::new(
        secs.parse().map_err(|_| "invalid seconds")?,
        nanos,
    ))
}
const KINDS: [NodeKind; 7] = [
    NodeKind::Evidence,
    NodeKind::Derived,
    NodeKind::ActionStart,
    NodeKind::ActionRun,
    NodeKind::ActionCommit,
    NodeKind::ActionOutcome,
    NodeKind::PhysicalSafety,
];
impl Scenario {
    pub fn encode(&self) -> String {
        let mut text = format!("DCRA_TRACE_V1 {}\n", self.seed);
        for node in &self.nodes {
            writeln!(
                text,
                "N {} {}",
                hex(&node.node_id),
                KINDS.iter().position(|k| *k == node.kind).unwrap()
            )
            .unwrap();
        }
        for rule in &self.rules {
            writeln!(
                text,
                "J {} {} {} {}",
                hex(&rule.justification_id),
                rule.threshold,
                hex(&rule.conclusion),
                rule.premises
                    .iter()
                    .map(|p| hex(p))
                    .collect::<Vec<_>>()
                    .join(",")
            )
            .unwrap();
        }
        for operation in &self.operations {
            match operation {
                Operation::Advance(at) => writeln!(text, "A {}", time(*at)).unwrap(),
                Operation::Update(a) => writeln!(
                    text,
                    "U {} {} {} {} {} {} {} {} {}",
                    hex(&a.evidence_id),
                    hex(&a.evidence_type),
                    hex(&a.predicate),
                    hex(&a.source_id),
                    a.version,
                    time(a.observed_at),
                    time(a.expires_at),
                    match a.status {
                        Status::Valid => "V",
                        Status::Unknown => "U",
                        Status::Invalid => "I",
                    },
                    a.payload_hash.as_ref().map_or("-".into(), |p| hex(p))
                )
                .unwrap(),
            }
        }
        text
    }
    pub fn decode(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let header: Vec<_> = lines
            .next()
            .ok_or("missing header")?
            .split_whitespace()
            .collect();
        if header.len() != 2 || header[0] != "DCRA_TRACE_V1" {
            return Err("unsupported trace header".into());
        }
        let mut scenario = Self {
            seed: header[1].parse().map_err(|_| "invalid seed")?,
            nodes: Vec::new(),
            rules: Vec::new(),
            operations: Vec::new(),
        };
        for line in lines {
            let f: Vec<_> = line.split_whitespace().collect();
            match f.as_slice() {
                ["N", id, kind] => scenario.nodes.push(Node::new(
                    unhex(id)?,
                    *KINDS
                        .get(kind.parse::<usize>().map_err(|_| "invalid node kind")?)
                        .ok_or("invalid node kind")?,
                )),
                ["J", id, k, conclusion, premises] => scenario.rules.push(Justification {
                    justification_id: unhex(id)?,
                    premises: premises.split(',').map(unhex).collect::<Result<_, _>>()?,
                    threshold: k.parse().map_err(|_| "invalid threshold")?,
                    conclusion: unhex(conclusion)?,
                }),
                ["A", at] => scenario
                    .operations
                    .push(Operation::Advance(parse_time(at)?)),
                [
                    "U",
                    id,
                    kind,
                    predicate,
                    source,
                    version,
                    observed,
                    expires,
                    status,
                    payload,
                ] => scenario.operations.push(Operation::Update(EvidenceAtom {
                    evidence_id: unhex(id)?,
                    evidence_type: unhex(kind)?,
                    predicate: unhex(predicate)?,
                    source_id: unhex(source)?,
                    version: version.parse().map_err(|_| "invalid version")?,
                    observed_at: parse_time(observed)?,
                    expires_at: parse_time(expires)?,
                    status: match *status {
                        "V" => Status::Valid,
                        "U" => Status::Unknown,
                        "I" => Status::Invalid,
                        _ => return Err("invalid status".into()),
                    },
                    payload_hash: if *payload == "-" {
                        None
                    } else {
                        Some(unhex(payload)?)
                    },
                })),
                _ => return Err(format!("invalid trace line: {line}")),
            }
        }
        Ok(scenario)
    }
}
