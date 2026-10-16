//! Bounded-state experiments with a private plant and independent scorer.
//! Safety conclusions are conditional on the configured discrete model/bounds.
//!
//! The hidden plant is not part of the policy-facing public API:
//! ```compile_fail
//! use dcra::model_v1::benchmark::TrueState;
//! ```
mod benchmark;
mod check;
mod estimate;
mod interval;
mod types;

pub use benchmark::{ExperimentReport, SCENARIOS, run_experiment};
pub use check::{CheckResult, Decision, check_action};
pub use estimate::{Estimator, Update};
pub use interval::Interval;
pub use types::*;

#[cfg(test)]
mod tests;
