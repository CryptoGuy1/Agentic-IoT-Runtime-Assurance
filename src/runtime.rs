//! Single-threaded deterministic evidence lifecycle with explicit audit events.
use crate::clock::ControlledClock;
use crate::evaluator::{Evaluation, Witness, evaluate_full};
use crate::evidence::{AssuranceStatus, AssuranceValue, EvidenceAtom};
use crate::graph::{AssuranceGraph, NodeKind};
pub use crate::incremental::EvaluationStats;
use crate::incremental::{Change, IncrementalEvaluator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationMode {
    Full,
    Incremental,
}
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    NotEvidenceNode(String),
    EmptyMetadata,
    DuplicateBatchEvidence(String),
    InvalidObservationTime,
    NonIncreasingVersion(String),
    ChangedIdentity(String),
    TimeWentBackwards,
    TimeOverflow,
    EpochOverflow,
}
impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "evidence update rejected: {self:?}")
    }
}
impl std::error::Error for RuntimeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    EvidenceUpdated {
        previous: Option<EvidenceAtom>,
        current: EvidenceAtom,
    },
    /// Explicit VALID -> UNKNOWN transition; no new observation version.
    EvidenceExpired { evidence_id: String, version: u64 },
    AssuranceChanged {
        node_id: String,
        previous: AssuranceValue,
        current: AssuranceValue,
    },
    WitnessChanged {
        node_id: String,
        previous: Option<Witness>,
        current: Option<Witness>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub at: Duration,
    pub epoch: u64,
    pub event: AuditEvent,
}

/// Optional host-performance diagnostics, excluded from semantic audit histories.
#[derive(Debug, Clone)]
pub struct EvaluationWork {
    pub at: Duration,
    pub epoch: u64,
    pub changed: Vec<String>,
    pub nodes: usize,
    pub rules: usize,
    pub affected_rules: usize,
    pub stats: EvaluationStats,
    pub elapsed_ns: u128,
}

#[derive(Debug)]
pub struct AssuranceRuntime {
    graph: AssuranceGraph,
    clock: ControlledClock,
    evidence: BTreeMap<String, EvidenceAtom>,
    epoch: u64,
    evaluation: Evaluation,
    audit: Vec<AuditEntry>,
    incremental: Option<IncrementalEvaluator>,
    last_evaluation_stats: EvaluationStats,
    operation_stats: EvaluationStats,
    work_log: Option<Vec<EvaluationWork>>,
}

impl AssuranceRuntime {
    pub fn new(graph: AssuranceGraph) -> Self {
        Self::with_mode(graph, EvaluationMode::Incremental)
    }

    pub fn with_mode(graph: AssuranceGraph, mode: EvaluationMode) -> Self {
        let evidence = BTreeMap::new();
        let incremental =
            (mode == EvaluationMode::Incremental).then(|| IncrementalEvaluator::new(&graph));
        let evaluation = match &incremental {
            Some(engine) => {
                let mut initial = Evaluation::default();
                engine.update(
                    &graph,
                    &evidence,
                    Duration::ZERO,
                    &mut initial,
                    graph.topological_order(),
                );
                initial
            }
            None => evaluate_full(&graph, &evidence, Duration::ZERO),
        };
        Self {
            graph,
            clock: ControlledClock::default(),
            evidence,
            epoch: 0,
            evaluation,
            audit: Vec::new(),
            incremental,
            last_evaluation_stats: EvaluationStats::default(),
            operation_stats: EvaluationStats::default(),
            work_log: None,
        }
    }
    pub fn enable_work_log(&mut self) {
        self.work_log.get_or_insert_with(Vec::new);
    }
    pub fn work_log(&self) -> &[EvaluationWork] {
        self.work_log.as_deref().unwrap_or(&[])
    }
    pub fn mode(&self) -> EvaluationMode {
        if self.incremental.is_some() {
            EvaluationMode::Incremental
        } else {
            EvaluationMode::Full
        }
    }
    /// Work in the last evaluation batch of the most recent accepted operation.
    /// Zero if that operation did not evaluate; rejected operations preserve counters.
    pub fn last_evaluation_stats(&self) -> EvaluationStats {
        self.last_evaluation_stats
    }
    /// Work summed across all expiry batches in the most recent accepted operation.
    pub fn operation_stats(&self) -> EvaluationStats {
        self.operation_stats
    }
    fn reset_stats(&mut self) {
        self.last_evaluation_stats = EvaluationStats::default();
        self.operation_stats = EvaluationStats::default();
    }
    pub fn now(&self) -> Duration {
        self.clock.now()
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn graph(&self) -> &AssuranceGraph {
        &self.graph
    }
    pub fn evidence(&self) -> &BTreeMap<String, EvidenceAtom> {
        &self.evidence
    }
    pub fn evaluation(&self) -> &Evaluation {
        &self.evaluation
    }
    pub fn audit(&self) -> &[AuditEntry] {
        &self.audit
    }

    /// Updates are atomic on validation failure. Versions must strictly increase
    /// per evidence ID; the ID's source, type and predicate remain stable.
    fn validate_atom(&self, atom: &EvidenceAtom) -> Result<(), RuntimeError> {
        if self.graph.nodes().get(&atom.evidence_id) != Some(&NodeKind::Evidence) {
            return Err(RuntimeError::NotEvidenceNode(atom.evidence_id.clone()));
        }
        if [&atom.evidence_type, &atom.predicate, &atom.source_id]
            .iter()
            .any(|s| s.trim().is_empty())
        {
            return Err(RuntimeError::EmptyMetadata);
        }
        if atom.observed_at > self.now() || atom.expires_at < atom.observed_at {
            return Err(RuntimeError::InvalidObservationTime);
        }
        if let Some(previous) = self.evidence.get(&atom.evidence_id) {
            if atom.version <= previous.version {
                return Err(RuntimeError::NonIncreasingVersion(atom.evidence_id.clone()));
            }
            if atom.source_id != previous.source_id
                || atom.predicate != previous.predicate
                || atom.evidence_type != previous.evidence_type
            {
                return Err(RuntimeError::ChangedIdentity(atom.evidence_id.clone()));
            }
        }
        Ok(())
    }

    /// Validate all distinct records before applying any. Action lifecycle reactions
    /// happen after the whole batch; ordinary per-record assurance audits remain.
    pub(crate) fn update_batch(&mut self, atoms: Vec<EvidenceAtom>) -> Result<(), RuntimeError> {
        let mut ids = std::collections::BTreeSet::new();
        let mut epochs = 0u64;
        for atom in &atoms {
            if !ids.insert(atom.evidence_id.clone()) {
                return Err(RuntimeError::DuplicateBatchEvidence(
                    atom.evidence_id.clone(),
                ));
            }
            self.validate_atom(atom)?;
            epochs = epochs
                .checked_add(
                    if atom.status == AssuranceStatus::Valid && atom.expires_at <= self.now() {
                        2
                    } else {
                        1
                    },
                )
                .ok_or(RuntimeError::EpochOverflow)?;
        }
        self.epoch
            .checked_add(epochs)
            .ok_or(RuntimeError::EpochOverflow)?;
        for atom in atoms {
            self.update(atom)
                .expect("batch prevalidated with distinct IDs and sufficient epochs");
        }
        Ok(())
    }

    pub fn update(&mut self, atom: EvidenceAtom) -> Result<(), RuntimeError> {
        self.validate_atom(&atom)?;
        let already_expired =
            atom.status == AssuranceStatus::Valid && atom.expires_at <= self.now();
        self.epoch
            .checked_add(if already_expired { 2 } else { 1 })
            .ok_or(RuntimeError::EpochOverflow)?;
        self.reset_stats();
        self.epoch += 1;
        let previous = self.evidence.insert(atom.evidence_id.clone(), atom.clone());
        self.record(AuditEvent::EvidenceUpdated {
            previous,
            current: atom.clone(),
        });
        if already_expired {
            self.expire(&atom.evidence_id);
        }
        self.reevaluate(&[atom.evidence_id]);
        Ok(())
    }

    pub fn advance(&mut self, elapsed: Duration) -> Result<(), RuntimeError> {
        let target = self
            .now()
            .checked_add(elapsed)
            .ok_or(RuntimeError::TimeOverflow)?;
        self.advance_to(target)
    }

    /// Process each deadline in chronological order, then evidence-ID order.
    /// Deadlines are selected from current observations: replacement cannot leave
    /// a stale timer that later expires the replacement observation.
    pub fn advance_to(&mut self, target: Duration) -> Result<(), RuntimeError> {
        if target < self.now() {
            return Err(RuntimeError::TimeWentBackwards);
        }
        let mut due: Vec<_> = self
            .evidence
            .values()
            .filter(|atom| atom.status == AssuranceStatus::Valid && atom.expires_at <= target)
            .map(|atom| (atom.expires_at, atom.evidence_id.clone()))
            .collect();
        due.sort();
        self.epoch
            .checked_add(due.len() as u64)
            .ok_or(RuntimeError::EpochOverflow)?;
        self.reset_stats();
        let mut events = due.into_iter().peekable();
        while let Some((deadline, id)) = events.next() {
            self.clock.advance(deadline.saturating_sub(self.now()));
            self.expire(&id);
            let mut dirty = vec![id];
            while events.peek().is_some_and(|(at, _)| *at == deadline) {
                let (_, id) = events.next().unwrap();
                self.expire(&id);
                dirty.push(id);
            }
            // All evidence expiring at this instant transitions before evaluation.
            self.reevaluate(&dirty);
        }
        self.clock.advance(target - self.now());
        Ok(())
    }

    fn expire(&mut self, id: &str) {
        self.epoch += 1;
        let atom = self.evidence.get_mut(id).unwrap();
        atom.status = AssuranceStatus::Unknown;
        let version = atom.version;
        self.record(AuditEvent::EvidenceExpired {
            evidence_id: id.into(),
            version,
        });
    }
    fn record(&mut self, event: AuditEvent) {
        self.audit.push(AuditEntry {
            at: self.now(),
            epoch: self.epoch,
            event,
        });
    }
    fn reevaluate(&mut self, dirty: &[String]) {
        let timer = self.work_log.as_ref().map(|_| std::time::Instant::now());
        let (changes, stats) = if let Some(engine) = &self.incremental {
            engine.update(
                &self.graph,
                &self.evidence,
                self.now(),
                &mut self.evaluation,
                dirty,
            )
        } else {
            let next = evaluate_full(&self.graph, &self.evidence, self.now());
            let mut changes = Vec::new();
            for (id, after) in &next.assurance_by_node {
                let before = self.evaluation.assurance_by_node[id];
                let old_witness = self.evaluation.preferred_witness_by_node.get(id).cloned();
                let new_witness = next.preferred_witness_by_node.get(id).cloned();
                if before != *after || old_witness != new_witness {
                    changes.push(Change {
                        id: id.clone(),
                        before,
                        after: *after,
                        old_witness,
                        new_witness,
                    });
                }
            }
            self.evaluation = next;
            (
                changes,
                EvaluationStats {
                    nodes_evaluated: self.graph.nodes().len(),
                    justifications_evaluated: self.graph.justifications().len(),
                },
            )
        };
        if let Some(timer) = timer {
            let elapsed_ns = timer.elapsed().as_nanos();
            let mut affected: std::collections::BTreeSet<_> = dirty.iter().cloned().collect();
            let mut rules = std::collections::BTreeSet::new();
            // Diagnostic reachability is outside the timed evaluator operation.
            for node in self.graph.topological_order() {
                for (id, rule) in self
                    .graph
                    .justifications()
                    .iter()
                    .filter(|(_, r)| &r.conclusion == node)
                {
                    if rule.premises.iter().any(|p| affected.contains(p)) {
                        rules.insert(id.clone());
                        affected.insert(node.clone());
                    }
                }
            }
            let record = EvaluationWork {
                at: self.now(),
                epoch: self.epoch,
                changed: dirty.to_vec(),
                nodes: self.graph.nodes().len(),
                rules: self.graph.justifications().len(),
                affected_rules: rules.len(),
                stats,
                elapsed_ns,
            };
            self.work_log.as_mut().unwrap().push(record);
        }
        self.last_evaluation_stats = stats;
        self.operation_stats.nodes_evaluated += stats.nodes_evaluated;
        self.operation_stats.justifications_evaluated += stats.justifications_evaluated;
        for change in changes {
            if change.before != change.after {
                self.record(AuditEvent::AssuranceChanged {
                    node_id: change.id.clone(),
                    previous: change.before,
                    current: change.after,
                });
            }
            if change.old_witness != change.new_witness {
                self.record(AuditEvent::WitnessChanged {
                    node_id: change.id,
                    previous: change.old_witness,
                    current: change.new_witness,
                });
            }
        }
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use crate::graph::Node;
    #[test]
    fn invalid_or_duplicate_batch_does_not_apply_its_valid_prefix() {
        let graph = AssuranceGraph::new(
            vec![
                Node::new("a", NodeKind::Evidence),
                Node::new("b", NodeKind::Evidence),
            ],
            vec![],
        )
        .unwrap();
        let mut runtime = AssuranceRuntime::new(graph);
        let atom = |id: &str| EvidenceAtom {
            evidence_id: id.into(),
            evidence_type: "sensor".into(),
            predicate: "high".into(),
            source_id: id.into(),
            version: 1,
            observed_at: Duration::ZERO,
            expires_at: Duration::from_secs(2),
            status: AssuranceStatus::Valid,
            payload_hash: None,
        };
        let mut invalid = atom("b");
        invalid.observed_at = Duration::from_secs(1);
        assert_eq!(
            runtime.update_batch(vec![atom("a"), invalid]),
            Err(RuntimeError::InvalidObservationTime)
        );
        assert_eq!(
            runtime.update_batch(vec![atom("a"), atom("a")]),
            Err(RuntimeError::DuplicateBatchEvidence("a".into()))
        );
        assert!(runtime.evidence().is_empty());
        assert!(runtime.audit().is_empty());
        assert_eq!(runtime.epoch(), 0);
        assert_eq!(runtime.operation_stats(), EvaluationStats::default());
    }
}
