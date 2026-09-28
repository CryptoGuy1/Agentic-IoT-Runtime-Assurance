//! Contracts, single-use authority, and a deterministic local execution boundary.
mod execution;
mod fake;
mod types;
pub use execution::ActionRuntime;
pub use fake::FakeActuator;
pub use types::*;
