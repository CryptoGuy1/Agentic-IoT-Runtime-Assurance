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
