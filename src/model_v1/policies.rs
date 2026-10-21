//! Explicit comparison schema. Historical policy IDs are not redefined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefinementPolicy {
    B0,
    B1,
    B2,
    B3,
    B4,
    B5,
    B6,
}
pub const POLICY_SCHEMA: &str = "refinement";
pub const REFINEMENT_POLICIES: [RefinementPolicy; 7] = [
    RefinementPolicy::B0,
    RefinementPolicy::B1,
    RefinementPolicy::B2,
    RefinementPolicy::B3,
    RefinementPolicy::B4,
    RefinementPolicy::B5,
    RefinementPolicy::B6,
];
impl RefinementPolicy {
    pub fn id(self) -> &'static str {
        match self {
            Self::B0 => "B0",
            Self::B1 => "B1",
            Self::B2 => "B2",
            Self::B3 => "B3",
            Self::B4 => "B4",
            Self::B5 => "B5",
            Self::B6 => "B6",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::B0 => "permission-only",
            Self::B1 => "static-point",
            Self::B2 => "fresh-point-admission",
            Self::B3 => "point-model-once",
            Self::B4 => "global-fresh-continuous",
            Self::B5 => "dependency-fresh-continuous",
            Self::B6 => "bounded-model-continuous",
        }
    }
    pub fn parse(id: &str) -> Result<Self, String> {
        REFINEMENT_POLICIES
            .into_iter()
            .find(|p| p.id() == id)
            .ok_or_else(|| "policy must be B0 through B6 in the refinement schema".into())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionBoundary {
    Start,
    Run,
    Commit,
    Outcome,
}
/// Information-only input; reference labels are supplied separately by fixtures.
#[derive(Clone, Copy, Debug)]
pub struct BoundaryInformation {
    pub phase: DecisionBoundary,
    pub phase_ready: bool,
    pub permission: bool,
    pub point_predicate: bool,
    pub relevant_fresh: bool,
    pub all_fresh: bool,
    pub point_model: bool,
    pub bounded_model: bool,
    pub recovery_feasible: bool,
    pub correlated_ack: bool,
    pub observed_target: bool,
}
/// Frozen policy contract evaluator used for decision-corpus tests. Execution
/// additionally retains the runtime's leases, identity and lifecycle checks.
pub fn policy_response(policy: RefinementPolicy, i: BoundaryInformation) -> bool {
    if i.phase == DecisionBoundary::Outcome {
        return i.phase_ready && i.correlated_ack && i.observed_target;
    }
    if !i.phase_ready || !i.permission {
        return false;
    }
    match policy {
        RefinementPolicy::B0 => true,
        RefinementPolicy::B1 => i.point_predicate,
        RefinementPolicy::B2 => {
            i.phase == DecisionBoundary::Run || (i.point_predicate && i.relevant_fresh)
        }
        RefinementPolicy::B3 => i.phase != DecisionBoundary::Start || i.point_model,
        RefinementPolicy::B4 => i.point_predicate && i.relevant_fresh && i.all_fresh,
        RefinementPolicy::B5 => i.point_predicate && i.relevant_fresh,
        RefinementPolicy::B6 => i.bounded_model && i.recovery_feasible,
    }
}
