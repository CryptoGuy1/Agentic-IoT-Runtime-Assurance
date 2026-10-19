//! Bounded-state experiments with a private plant and independent scorer.
//! Safety conclusions are conditional on the configured discrete model/bounds.
//!
//! The hidden plant is not part of the policy-facing public API:
//! ```compile_fail
//! use dcra::model_v1::benchmark::TrueState;
//! ```
mod benchmark;
mod check;
mod claims;
mod closed_loop;
mod estimate;
mod execution_score;
mod integration;
mod interval;
mod planning;
mod policies;
mod types;

pub use benchmark::{ExperimentReport, SCENARIOS, run_experiment};
pub use check::{CheckResult, Decision, check_action};
pub use claims::{
    CLAIM_SCHEMA, ClaimConfiguration, ObservationReference, PhysicalCertificate,
    PhysicalClaimBinding, PhysicalPredicate, substitute_physical_claims,
};
pub use closed_loop::{
    CLOSED_LOOP_SCENARIOS, CLOSED_LOOP_SCHEMA, ClosedLoopReport, RunOptions, run_closed_loop,
    run_closed_loop_with_options,
};
pub use estimate::{Estimator, Update};
pub use execution_score::{AppliedCommandScore, ExecutionScore};
pub use integration::{IntegratedError, ModelAudit, ModelBinding, ModelRuntime, PhysicalCommand};
pub use interval::Interval;
pub use planning::{GoalSupervisor, PlanningIntent};
pub use policies::{
    BoundaryInformation, DecisionBoundary, POLICY_SCHEMA, REFINEMENT_POLICIES, RefinementPolicy,
    policy_response,
};
pub use types::*;

#[cfg(test)]
mod tests;
