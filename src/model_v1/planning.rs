//! Bounded deterministic goal supervisor. It reads policy knowledge/history only.
use super::ModelRuntime;
use crate::actions::ActionState;
use std::time::Duration;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanningIntent {
    Wait,
    Try(String),
    OpenThenRetry,
    Exhausted,
}
#[derive(Debug, Default)]
pub struct GoalSupervisor {
    attempts: usize,
    last_attempt: Option<Duration>,
    replanned: bool,
    exhausted_reported: bool,
}
impl GoalSupervisor {
    pub fn decide(&mut self, runtime: &ModelRuntime) -> PlanningIntent {
        if runtime
            .runtime()
            .instances()
            .get("fan-2")
            .or_else(|| runtime.runtime().instances().get("fan-1"))
            .is_some_and(|i| i.state == ActionState::Completed)
        {
            return PlanningIntent::Wait;
        }
        let now = runtime.runtime().now();
        if self.attempts >= 4 || now >= Duration::from_secs(12) {
            if !self.exhausted_reported {
                self.exhausted_reported = true;
                return PlanningIntent::Exhausted;
            }
            return PlanningIntent::Wait;
        }
        if self
            .last_attempt
            .is_some_and(|at| now < at + Duration::from_millis(500))
        {
            return PlanningIntent::Wait;
        }
        let instances = runtime.runtime().instances();
        if instances
            .get("fan-1")
            .is_some_and(|i| i.state == ActionState::Cancelled)
            && !self.replanned
        {
            let permission =
                runtime.runtime().assurance().evaluation().assurance_by_node["permission"].status();
            if permission != crate::evidence::AssuranceStatus::Valid {
                return PlanningIntent::Wait;
            }
            self.replanned = true;
            return PlanningIntent::OpenThenRetry;
        }
        for id in ["open-1", "fan-2", "fan-1"] {
            if let Some(i) = instances.get(id) {
                if !matches!(i.state, ActionState::Pending | ActionState::Ready) {
                    continue;
                }
                if id == "fan-2"
                    && instances
                        .get("open-1")
                        .is_none_or(|p| p.state != ActionState::Completed)
                {
                    continue;
                }
                self.attempts += 1;
                self.last_attempt = Some(now);
                return PlanningIntent::Try(id.into());
            }
        }
        PlanningIntent::Wait
    }
    pub fn attempts(&self) -> usize {
        self.attempts
    }
    pub fn note_initial_attempt(&mut self, at: Duration) {
        self.attempts += 1;
        self.last_attempt = Some(at);
    }
}
