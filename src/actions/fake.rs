use super::{ActionError, ActionRuntime, ActuatorCommand, ActuatorResult};
use crate::evidence::AssuranceStatus;
use std::collections::BTreeMap;

/// A deterministic test adapter. The caller supplies observations; this helper
/// never invents successful physical evidence or advances the clock.
pub struct FakeActuator;
impl FakeActuator {
    pub fn respond_next(
        runtime: &mut ActionRuntime,
        acknowledged: Option<bool>,
        completed: bool,
        observed_state: BTreeMap<String, AssuranceStatus>,
    ) -> Result<Option<ActuatorCommand>, ActionError> {
        let Some(command) = runtime.take_command() else {
            return Ok(None);
        };
        runtime.submit_result(ActuatorResult {
            command_id: command.command_id.clone(),
            sequence: 1,
            acknowledged,
            observed_state,
            completed,
            observed_at: runtime.now(),
        })?;
        Ok(Some(command))
    }
}
