use crate::evaluator::Witness;
use crate::evidence::AssuranceStatus;
use crate::runtime::RuntimeError;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interruptibility {
    Preemptible,
    CompletionSafe,
    Irreversible,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryType {
    Reversible,
    Compensatable,
    Irreversible,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ControllerCapabilities {
    pub safe_abort: bool,
    pub certified_complete: bool,
    pub forward_mitigation: bool,
    pub compensate: bool,
}
/// Only these named observations can be written by actuator responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeBindings {
    pub acknowledgement: String,
    pub observed: BTreeMap<String, String>,
    pub max_age: Duration,
}
impl OutcomeBindings {
    pub(crate) fn ids(&self) -> BTreeSet<String> {
        std::iter::once(self.acknowledgement.clone())
            .chain(self.observed.values().cloned())
            .collect()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDefinition {
    pub action_id: String,
    pub start_contract: String,
    pub run_contract: Option<String>,
    pub commit_contract: Option<String>,
    pub outcome_contract: String,
    pub min_duration: Duration,
    pub max_duration: Duration,
    pub interruptibility: Interruptibility,
    pub recovery_type: RecoveryType,
    pub resources_read: BTreeSet<String>,
    pub resources_written: BTreeSet<String>,
    pub max_start_lease: Duration,
    pub controllers: ControllerCapabilities,
    pub outcomes: OutcomeBindings,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState {
    Pending,
    Ready,
    Leased,
    Executing,
    Committed,
    Aborting,
    Recovering,
    Completed,
    Failed,
    Cancelled,
}
impl ActionState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
    pub(crate) fn is_running(self) -> bool {
        matches!(self, Self::Executing | Self::Committed)
    }
    pub(crate) fn occupies_bindings(self) -> bool {
        matches!(
            self,
            Self::Leased | Self::Executing | Self::Committed | Self::Aborting | Self::Recovering
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionInstance {
    pub instance_id: String,
    pub action_id: String,
    pub plan_id: String,
    /// Original execution attribution, never rewritten by a revision.
    pub plan_version: u64,
    pub state: ActionState,
    pub created_at: Duration,
    pub started_at: Option<Duration>,
    pub committed_at: Option<Duration>,
    pub completed_at: Option<Duration>,
    pub active_lease_id: Option<String>,
    pub active_command_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub plan_id: String,
    pub version: u64,
    /// Instance ID -> action-definition ID.
    pub actions: BTreeMap<String, String>,
    /// (predecessor instance ID, successor instance ID).
    pub precedence: BTreeSet<(String, String)>,
    /// Stamped by register_plan; input value is ignored.
    pub created_at: Duration,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionPhase {
    Start,
    Commit,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseRevocation {
    Expired,
    EvidenceChanged,
    PlanChanged,
    ContractChanged,
    Replaced,
    ActionStopped,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseToken {
    pub lease_id: String,
    pub nonce: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssuranceLease {
    pub lease_id: String,
    pub action_instance_id: String,
    pub phase: ActionPhase,
    pub plan_version: u64,
    pub assurance_epoch: u64,
    pub issued_at: Duration,
    pub expires_at: Duration,
    pub witness: Witness,
    pub evidence_versions: BTreeMap<String, u64>,
    pub nonce: String,
    pub consumed: bool,
    pub revoked: Option<LeaseRevocation>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Start,
    Commit,
    SafeAbort,
    CertifiedComplete,
    ForwardMitigation,
    Recover,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActuatorCommand {
    pub command_id: String,
    pub instance_id: String,
    pub kind: CommandKind,
    pub issued_at: Duration,
    pub delivered: bool,
    pub last_result_sequence: Option<u64>,
    pub last_observed_at: Option<Duration>,
    pub completion_reported: bool,
    pub last_acknowledged: Option<bool>,
}
/// Sequence numbers allow a later acknowledgement/completion after an earlier
/// unknown or progress report, while rejecting duplicates and reordered reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActuatorResult {
    pub command_id: String,
    pub sequence: u64,
    pub acknowledged: Option<bool>,
    pub observed_state: BTreeMap<String, AssuranceStatus>,
    pub completed: bool,
    pub observed_at: Duration,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionKind {
    Eligible,
    Revalidate,
    BlockAndReplan,
    Timeout,
    LeaseRejected,
    RecoveryRequested(CommandKind),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDecision {
    pub instance_id: String,
    pub phase: Option<ActionPhase>,
    pub kind: DecisionKind,
    pub status: Option<AssuranceStatus>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionEvent {
    PlanRegistered {
        plan_id: String,
        version: u64,
    },
    PlanVersionChanged {
        plan_id: String,
        previous: u64,
        current: u64,
    },
    StateChanged {
        instance_id: String,
        previous: ActionState,
        current: ActionState,
    },
    LeaseIssued {
        lease_id: String,
    },
    LeaseRevoked {
        lease_id: String,
        reason: LeaseRevocation,
    },
    LeaseConsumed {
        lease_id: String,
    },
    CommandQueued {
        command_id: String,
    },
    CommandDelivered {
        command_id: String,
    },
    ResultAccepted {
        command_id: String,
        sequence: u64,
    },
    Decision(ActionDecision),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionAuditEntry {
    pub at: Duration,
    pub epoch: u64,
    pub event: ActionEvent,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    Configuration(String),
    UnknownPlan,
    UnknownInstance,
    DuplicateIdentity,
    PlanVersion,
    DependenciesIncomplete,
    StartNotAcknowledged,
    BindingsBusy,
    IllegalTransition,
    ContractUnavailable(AssuranceStatus),
    UnknownLease,
    WrongLease,
    RevokedLease(LeaseRevocation),
    ConsumedLease,
    UnknownCommand,
    UndeliveredCommand,
    SupersededCommand,
    StaleResult,
    InvalidResult,
    ReservedEvidence,
    TimeOverflow,
    Evidence(RuntimeError),
}
impl From<RuntimeError> for ActionError {
    fn from(e: RuntimeError) -> Self {
        Self::Evidence(e)
    }
}
impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "action runtime: {self:?}")
    }
}
impl std::error::Error for ActionError {}
