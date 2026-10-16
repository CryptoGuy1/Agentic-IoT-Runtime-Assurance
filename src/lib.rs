//! Evidence assurance and action lifecycle, with full and incremental evaluation.
//!
//! This models declared evidence support, not a guarantee of physical safety.

pub mod clock;
pub mod evidence;

pub mod evaluator;
pub mod graph;
pub mod runtime;

mod incremental;

pub mod actions;

pub mod simulation;

/// Isolated Model v1 experiment; does not replace the existing simulation.
pub mod model_v1;
