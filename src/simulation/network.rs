//! Deterministic simulated transport. No sockets, wall-clock scheduling or hidden retries.
use std::time::Duration;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Sensor,
    Cloud,
    Authority,
    Actuator,
    Result,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkConfig {
    pub name: String,
    pub delay_ms: u64,
    pub jitter_ms: u64,
    pub loss_per_mille: u16,
    pub duplicate_per_mille: u16,
    pub reorder_ms: u64,
    pub authority_delay_ms: u64,
    pub actuator_delay_ms: u64,
    pub partition: Option<(u64, u64)>,
    pub partition_all_links: bool,
}
impl Default for NetworkConfig {
    fn default() -> Self {
        Self::profile("none").unwrap()
    }
}
impl NetworkConfig {
    pub fn profile(name: &str) -> Result<Self, String> {
        let mut c = Self {
            name: name.into(),
            delay_ms: 0,
            jitter_ms: 0,
            loss_per_mille: 0,
            duplicate_per_mille: 0,
            reorder_ms: 0,
            authority_delay_ms: 0,
            actuator_delay_ms: 0,
            partition: None,
            partition_all_links: false,
        };
        match name {
            "none"=>{}, "delay"=>{c.delay_ms=40;c.authority_delay_ms=80;},
            "loss"=>c.loss_per_mille=200,
            "duplicate"=>c.duplicate_per_mille=1000,
            "reorder"=>c.reorder_ms=750,
            "partition"=>c.partition=Some((500,3500)),
            "compound"=>{c.delay_ms=20;c.jitter_ms=80;c.loss_per_mille=100;c.duplicate_per_mille=200;c.reorder_ms=650;c.authority_delay_ms=50;c.partition=Some((1200,2200));},
            "stale-lease"=>c.authority_delay_ms=1100,
            "slow-controller"=>c.actuator_delay_ms=600,
            _=>return Err("network must be none, delay, loss, duplicate, reorder, partition, compound, stale-lease or slow-controller".into()),
        }
        Ok(c)
    }
    pub fn encode(&self) -> String {
        let (a, b) = self.partition.unwrap_or((0, 0));
        format!(
            "{} {} {} {} {} {} {} {} {} {} {}",
            self.name,
            self.delay_ms,
            self.jitter_ms,
            self.loss_per_mille,
            self.duplicate_per_mille,
            self.reorder_ms,
            self.authority_delay_ms,
            self.actuator_delay_ms,
            a,
            b,
            u8::from(self.partition_all_links)
        )
    }
    pub fn decode(s: &str) -> Result<Self, String> {
        let fields: Vec<_> = s.split_whitespace().collect();
        if fields.len() != 11 {
            return Err("invalid network configuration".into());
        }
        let nums: Vec<u64> = fields[1..]
            .iter()
            .map(|n| n.parse().map_err(|_| "invalid network number".to_string()))
            .collect::<Result<_, _>>()?;
        let c = Self {
            name: fields[0].into(),
            delay_ms: nums[0],
            jitter_ms: nums[1],
            loss_per_mille: nums[2].try_into().map_err(|_| "invalid probability")?,
            duplicate_per_mille: nums[3].try_into().map_err(|_| "invalid probability")?,
            reorder_ms: nums[4],
            authority_delay_ms: nums[5],
            actuator_delay_ms: nums[6],
            partition: if nums[7] == 0 && nums[8] == 0 {
                None
            } else {
                Some((nums[7], nums[8]))
            },
            partition_all_links: match nums[9] {
                0 => false,
                1 => true,
                _ => return Err("invalid partition flag".into()),
            },
        };
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.loss_per_mille > 1000
            || self.duplicate_per_mille > 1000
            || self.partition.is_some_and(|(a, b)| a >= b)
            || [
                self.delay_ms,
                self.jitter_ms,
                self.reorder_ms,
                self.authority_delay_ms,
                self.actuator_delay_ms,
            ]
            .iter()
            .any(|v| *v > 60_000)
        {
            return Err("invalid network probability, delay or partition interval".into());
        }
        Ok(())
    }
    pub fn partitioned(&self, link: Link, at: Duration) -> bool {
        (self.partition_all_links || matches!(link, Link::Cloud | Link::Authority))
            && self.partition.is_some_and(|(a, b)| {
                at >= Duration::from_millis(a) && at < Duration::from_millis(b)
            })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transmission {
    pub link: Link,
    pub key: String,
    pub sent_at: Duration,
    pub arrivals: Vec<Duration>,
    pub dropped: Option<String>,
}
#[derive(Debug, Clone)]
pub struct Network {
    pub config: NetworkConfig,
    seed: u64,
    pub transmissions: Vec<Transmission>,
}
impl Network {
    pub fn new(config: NetworkConfig, seed: u64) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            config,
            seed,
            transmissions: Vec::new(),
        })
    }
    /// Keyed random draws prevent a different action count in one policy from
    /// shifting the fault stream experienced by unrelated sensor samples.
    pub fn send(&mut self, link: Link, key: &str, at: Duration) -> Vec<Duration> {
        let draw = |salt: &str| {
            let mut h = 0xcbf29ce484222325u64 ^ self.seed;
            for b in format!("{link:?}:{key}:{salt}").bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x100000001b3);
            }
            h
        };
        let c = &self.config;
        let drop = if c.partitioned(link, at) {
            Some("partition at send".into())
        } else if draw("loss") % 1000 < u64::from(c.loss_per_mille) {
            Some("packet loss".into())
        } else {
            None
        };
        let mut arrivals = Vec::new();
        if drop.is_none() {
            let delay = c.delay_ms
                + if link == Link::Authority {
                    c.authority_delay_ms
                } else {
                    0
                };
            let delay = delay
                + if link == Link::Actuator {
                    c.actuator_delay_ms
                } else {
                    0
                };
            let jitter = draw("jitter") % (c.jitter_ms + 1);
            let reorder = if draw("reorder").is_multiple_of(2) {
                c.reorder_ms
            } else {
                0
            };
            let arrival = at + Duration::from_millis(delay + jitter + reorder);
            arrivals.push(arrival);
            if draw("duplicate") % 1000 < u64::from(c.duplicate_per_mille) {
                arrivals.push(arrival + Duration::from_millis(5));
            }
        }
        self.transmissions.push(Transmission {
            link,
            key: key.into(),
            sent_at: at,
            arrivals: arrivals.clone(),
            dropped: drop,
        });
        arrivals
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ablation {
    #[default]
    None,
    NoAlternatives,
    NoRun,
    NoCommit,
    NoPhysical,
}
impl std::str::FromStr for Ablation {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "none" => Ok(Self::None),
            "no-alternatives" => Ok(Self::NoAlternatives),
            "no-run" => Ok(Self::NoRun),
            "no-commit" => Ok(Self::NoCommit),
            "no-physical" => Ok(Self::NoPhysical),
            _ => Err(
                "ablation must be none, no-alternatives, no-run, no-commit or no-physical".into(),
            ),
        }
    }
}
impl Ablation {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NoAlternatives => "no-alternatives",
            Self::NoRun => "no-run",
            Self::NoCommit => "no-commit",
            Self::NoPhysical => "no-physical",
        }
    }
}
