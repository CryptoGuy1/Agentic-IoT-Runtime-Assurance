//! Deliberately small building model; observations and ground truth are separate.
use crate::evidence::{AssuranceStatus as Status, AssuranceValue};
use std::collections::BTreeMap;
use std::time::Duration;

pub const ACTIONS: [&str; 7] = [
    "alarm",
    "fan",
    "damper",
    "door",
    "arm",
    "discharge",
    "light",
];
pub const SENSORS: [&str; 15] = [
    "s1",
    "s2",
    "thermal",
    "fan_healthy",
    "pressure_safe",
    "damper_open",
    "fan_off",
    "occupancy_clear",
    "egress_clear",
    "alarm_active",
    "suppression_armed",
    "path_healthy",
    "operator_approved",
    "agent_available",
    "permission",
];
pub fn duration(action: &str) -> Duration {
    Duration::from_millis(match action {
        "alarm" | "light" => 100,
        "fan" => 3000,
        "damper" => 2000,
        "door" => 2500,
        "arm" => 1000,
        "discharge" => 200,
        _ => unreachable!(),
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingState {
    pub hazard: [bool; 3],
    pub occupied: [bool; 3],
    pub fan_high: bool,
    pub fan_starting: bool,
    pub damper_closed: bool,
    pub damper_closing: bool,
    pub door_closed: bool,
    pub door_closing: bool,
    pub alarm_active: bool,
    pub light_on: bool,
    pub suppression_armed: bool,
    pub suppression_discharged: bool,
    pub pressure: i32,
    pub egress_clear: bool,
    pub fan_healthy: bool,
    pub path_healthy: bool,
    pub operator_approved: bool,
    pub agent_available: bool,
}
impl Default for BuildingState {
    fn default() -> Self {
        Self {
            hazard: [true; 3],
            occupied: [false; 3],
            fan_high: false,
            fan_starting: false,
            damper_closed: false,
            damper_closing: false,
            door_closed: false,
            door_closing: false,
            alarm_active: false,
            light_on: false,
            suppression_armed: false,
            suppression_discharged: false,
            pressure: 100,
            egress_clear: true,
            fan_healthy: true,
            path_healthy: true,
            operator_approved: true,
            agent_available: true,
        }
    }
}
impl BuildingState {
    pub fn sample(&self, id: &str) -> Status {
        let value = match id {
            "s1" | "s2" | "thermal" => self.hazard[1],
            "fan_healthy" => self.fan_healthy,
            "pressure_safe" => self.pressure >= 50,
            "damper_open" => !self.damper_closed && !self.damper_closing,
            "fan_off" => !self.fan_high && !self.fan_starting,
            "occupancy_clear" => !self.occupied[1],
            "egress_clear" => self.egress_clear,
            "alarm_active" => self.alarm_active,
            "suppression_armed" => self.suppression_armed,
            "path_healthy" => self.path_healthy,
            "operator_approved" => self.operator_approved,
            "agent_available" => self.agent_available,
            "permission" => true,
            _ => return Status::Unknown,
        };
        if value {
            Status::Valid
        } else {
            Status::Invalid
        }
    }
    pub fn violations(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.suppression_discharged && self.occupied[1] {
            v.push("suppression with occupant present");
        }
        if self.pressure < 50 {
            v.push("zone 2 pressure below minimum");
        }
        if self.door_closing && !self.egress_clear {
            v.push("door closing across blocked egress");
        }
        if self.fan_high && self.damper_closed {
            v.push("fan HIGH with damper CLOSED");
        }
        v
    }
    pub fn finish(&mut self, action: &str) {
        match action {
            "alarm" => self.alarm_active = true,
            "fan" => {
                self.fan_high = true;
                self.fan_starting = false;
            }
            "damper" => {
                self.damper_closed = true;
                self.damper_closing = false;
            }
            "door" => {
                self.door_closed = true;
                self.door_closing = false;
            }
            "arm" => self.suppression_armed = true,
            "discharge" => self.suppression_discharged = true,
            "light" => self.light_on = true,
            _ => unreachable!(),
        }
    }
    pub fn abort(&mut self, action: &str) {
        match action {
            "fan" => {
                self.fan_starting = false;
                self.fan_high = false;
            }
            "door" => self.door_closing = false,
            "damper" => self.damper_closing = false,
            "arm" => self.suppression_armed = false,
            _ => {}
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateEstimate(pub BTreeMap<String, (Status, Duration)>);
impl StateEstimate {
    pub fn status(&self, id: &str, now: Duration) -> Status {
        self.0.get(id).map_or(Status::Unknown, |(s, t)| {
            if *s == Status::Valid && now >= *t {
                Status::Unknown
            } else {
                *s
            }
        })
    }
}
pub trait PhysicalSafetyOracle {
    fn evaluate(
        &self,
        action: &str,
        state: &StateEstimate,
        concurrent: &[String],
        now: Duration,
    ) -> AssuranceValue;
}
pub struct BuildingOracle;
impl PhysicalSafetyOracle for BuildingOracle {
    fn evaluate(
        &self,
        action: &str,
        state: &StateEstimate,
        concurrent: &[String],
        now: Duration,
    ) -> AssuranceValue {
        if concurrent.iter().any(|other| !joint_safe(action, other)) {
            return AssuranceValue::Invalid;
        }
        let required = physical_inputs(action);
        let mut horizon = now.saturating_add(Duration::from_secs(1));
        let mut unknown = false;
        for id in required {
            match state.status(id, now) {
                Status::Invalid => return AssuranceValue::Invalid,
                Status::Unknown => unknown = true,
                Status::Valid => horizon = horizon.min(state.0[*id].1),
            }
        }
        if unknown {
            AssuranceValue::Unknown
        } else {
            AssuranceValue::Valid { horizon }
        }
    }
}
pub fn physical_inputs(action: &str) -> &'static [&'static str] {
    match action {
        "fan" => &["pressure_safe", "damper_open"],
        "damper" => &["fan_off"],
        "door" => &["egress_clear", "occupancy_clear"],
        "discharge" => &["occupancy_clear"],
        _ => &[],
    }
}
pub fn joint_safe(a: &str, b: &str) -> bool {
    a != b && !matches!((a, b), ("fan", "damper") | ("damper", "fan"))
}
/// Stable greedy inclusion-maximal compatible subset; no maximum-cardinality claim.
pub fn supported_subset(candidates: &[String], running: &[String]) -> Vec<String> {
    let mut sorted = candidates.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut selected = Vec::new();
    for a in sorted {
        if running
            .iter()
            .chain(selected.iter())
            .all(|b| joint_safe(&a, b))
        {
            selected.push(a);
        }
    }
    selected
}
