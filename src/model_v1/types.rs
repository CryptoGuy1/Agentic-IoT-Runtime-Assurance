use super::{Interval, interval::p};
use std::time::Duration;

pub const MODEL_VERSION: &str = "evidra-model-v1.1";
pub const TICK: Duration = Duration::from_millis(100);
pub(crate) fn grid(t: Duration) -> bool {
    t.as_nanos().is_multiple_of(TICK.as_nanos())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolSet {
    False,
    True,
    Either,
}
impl BoolSet {
    pub fn contains(self, value: bool) -> bool {
        self == Self::Either || (self == Self::True) == value
    }
    pub(crate) fn values(self) -> impl Iterator<Item = bool> {
        [false, true].into_iter().filter(move |v| self.contains(*v))
    }
    pub(crate) fn intersect(self, value: bool) -> Option<Self> {
        self.contains(value)
            .then_some(if value { Self::True } else { Self::False })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Variable {
    Hazard,
    Pressure,
    Fan,
    Door,
    Occupancy,
    Alarm,
}
impl Variable {
    pub const ALL: [Self; 6] = [
        Self::Hazard,
        Self::Pressure,
        Self::Fan,
        Self::Door,
        Self::Occupancy,
        Self::Alarm,
    ];
    pub fn period(self) -> Duration {
        Duration::from_millis(if self == Self::Hazard { 1000 } else { 500 })
    }
    pub fn max_age(self) -> Duration {
        Duration::from_millis(match self {
            Self::Hazard => 2500,
            Self::Fan | Self::Occupancy => 1000,
            _ => 1500,
        })
    }
    pub fn error(self) -> Option<f64> {
        match self {
            Self::Hazard => Some(0.04),
            Self::Pressure | Self::Fan => Some(0.03),
            Self::Door => Some(0.02),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reading {
    Number(f64),
    Boolean(bool),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Trusted,
    Missing,
    Inconsistent,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub id: String,
    pub source: String,
    pub variable: Variable,
    pub value: Reading,
    pub error: Option<f64>,
    pub generated_at: Duration,
    pub received_at: Duration,
    pub max_age: Duration,
    pub version: u64,
    pub quality: Quality,
}
impl Observation {
    pub(crate) fn validate(&self, now: Duration) -> Result<(), String> {
        if self.id.is_empty()
            || self.source.is_empty()
            || self.version == 0
            || self.max_age.is_zero()
            || !grid(self.generated_at)
            || self.generated_at > self.received_at
            || self.received_at > now
            || self.generated_at.checked_add(self.max_age).is_none()
        {
            return Err("invalid observation identity, version, time or age".into());
        }
        match (self.variable, self.value, self.error) {
            (Variable::Occupancy | Variable::Alarm, Reading::Boolean(_), None) => Ok(()),
            (
                Variable::Hazard | Variable::Pressure | Variable::Fan | Variable::Door,
                Reading::Number(v),
                Some(e),
            ) if v.is_finite()
                && e.is_finite()
                && (0.0..=1.0).contains(&e)
                && v >= if self.variable == Variable::Pressure {
                    -1.0
                } else {
                    0.0
                }
                && v <= 1.0 =>
            {
                Ok(())
            }
            _ => Err(
                "observation needs a correctly typed, in-range value and declared error bound"
                    .into(),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    AlarmOn,
    FanStart,
    FanStop,
    DoorClose,
    DoorOpen,
}
impl Action {
    pub fn horizon(self) -> Duration {
        Duration::from_secs(match self {
            Self::AlarmOn => 2,
            Self::FanStart => 5,
            Self::FanStop => 6,
            _ => 4,
        })
    }
    pub fn deadline(self) -> Duration {
        if self == Self::FanStart {
            Duration::from_secs(4)
        } else {
            self.horizon()
        }
    }
    pub(crate) fn device(self) -> u8 {
        match self {
            Self::AlarmOn => 0,
            Self::FanStart | Self::FanStop => 1,
            _ => 2,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: u64,
    pub action: Action,
    pub issued_at: Duration,
    /// Delivery bound includes the device's wait for a 100 ms application tick.
    pub max_delay: Duration,
    pub may_drop: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    pub fan: bool,
    pub door: i8,
    pub alarm: bool,
}
impl Inputs {
    pub(crate) fn validate(self) -> Result<(), String> {
        if (-1..=1).contains(&self.door) {
            Ok(())
        } else {
            Err("door input must be -1, 0 or 1".into())
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dynamics {
    pub hazard_decay: f64,
    pub fan_removal: f64,
    pub fan_tau: f64,
    pub pressure_decay: f64,
    pub pressure_coupling: f64,
    pub door_speed: f64,
}
impl Default for Dynamics {
    fn default() -> Self {
        Self {
            hazard_decay: 0.02,
            fan_removal: 0.20,
            fan_tau: 1.5,
            pressure_decay: 0.60,
            pressure_coupling: 0.55,
            door_speed: 0.40,
        }
    }
}
impl Dynamics {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if [
            self.hazard_decay,
            self.fan_removal,
            self.pressure_decay,
            self.pressure_coupling,
            self.door_speed,
        ]
        .into_iter()
        .all(|v| v.is_finite() && (0.0..=2.0).contains(&v))
            && self.fan_tau.is_finite()
            && (0.5..=10.0).contains(&self.fan_tau)
        {
            Ok(())
        } else {
            Err("model parameters outside supported finite domain".into())
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Assumptions {
    pub dynamics: Dynamics,
    pub disturbance_scale: f64,
    pub source: Interval,
    pub faults_may_start: bool,
    /// Declared occupancy stability assumption; never inferred from hidden events.
    pub occupancy_stable_until: Duration,
}
impl Default for Assumptions {
    fn default() -> Self {
        Self {
            dynamics: Dynamics::default(),
            disturbance_scale: 1.0,
            source: Interval::new(0.0, 0.08).unwrap(),
            faults_may_start: true,
            occupancy_stable_until: Duration::ZERO,
        }
    }
}
impl Assumptions {
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.dynamics.validate()?;
        if !self.disturbance_scale.is_finite()
            || !(0.0..=3.0).contains(&self.disturbance_scale)
            || !self.source.subset_of(Interval::new(0.0, 0.16).unwrap())
            || !grid(self.occupancy_stable_until)
        {
            return Err("invalid disturbance, source or occupancy assumption".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StateBox {
    pub at: Duration,
    pub hazard: Interval,
    pub pressure: Interval,
    pub fan: Interval,
    pub door: Interval,
    pub occupied: BoolSet,
    pub alarm: BoolSet,
    pub fan_failed: BoolSet,
    pub door_jammed: BoolSet,
    pub response_required: BoolSet,
}
impl StateBox {
    pub fn unknown() -> Self {
        Self {
            at: Duration::ZERO,
            hazard: Interval::new(0.0, 1.0).unwrap(),
            pressure: Interval::new(-1.0, 1.0).unwrap(),
            fan: Interval::new(0.0, 1.0).unwrap(),
            door: Interval::new(0.0, 1.0).unwrap(),
            occupied: BoolSet::Either,
            alarm: BoolSet::Either,
            fan_failed: BoolSet::Either,
            door_jammed: BoolSet::Either,
            response_required: BoolSet::Either,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        let unit = Interval::new(0.0, 1.0).unwrap();
        if !grid(self.at)
            || ![self.hazard, self.fan, self.door]
                .into_iter()
                .all(|x| x.subset_of(unit))
            || !self.pressure.subset_of(Interval::new(-1.0, 1.0).unwrap())
        {
            Err("invalid state box".into())
        } else {
            Ok(())
        }
    }
    pub(crate) fn refresh_response(&mut self) {
        self.response_required = if self.hazard.lo() >= 0.60 {
            BoolSet::True
        } else if self.hazard.hi() <= 0.30 {
            BoolSet::False
        } else if (self.hazard.hi() >= 0.60 && self.response_required != BoolSet::True)
            || (self.hazard.lo() <= 0.30 && self.response_required != BoolSet::False)
        {
            BoolSet::Either
        } else {
            self.response_required
        };
    }
    pub fn point_projection(&self) -> Self {
        let mut s = self.clone();
        s.hazard = p(s.hazard.midpoint());
        s.pressure = p(s.pressure.midpoint());
        s.fan = p(s.fan.midpoint());
        s.door = p(s.door.midpoint());
        // Deliberately optimistic diagnostic comparator, explicitly not a safety estimate.
        for v in [
            &mut s.occupied,
            &mut s.alarm,
            &mut s.fan_failed,
            &mut s.door_jammed,
            &mut s.response_required,
        ] {
            if *v == BoolSet::Either {
                *v = BoolSet::False;
            }
        }
        s
    }
}
