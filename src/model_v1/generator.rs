//! Development generator. Independent parameter draws, separate from plant noise.
//! No command decisions, scorer results or held-out observations enter generation.
pub const BUILDING_GENERATOR_VERSION: &str = "building-boundary-v1";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingVariant {
    pub generator_seed: u64,
    pub no_invalidation: bool,
    pub propose_ms: u64,
    pub initial_pressure: f64,
    pub initial_door: f64,
    pub sensor_delay_ms: u64,
    pub pressure_gap_start_ms: u64,
    pub pressure_gap_end_ms: u64,
    pub revoke_ms: Option<u64>,
}

impl BuildingVariant {
    /// Uniform draws on explicitly finite grids. Timing is not selected relative
    /// to a particular race window; do not condition on an observed disagreement.
    pub fn draw(seed: u64, no_invalidation: bool) -> Self {
        let mut rng = seed ^ 0x424f_554e_4441_5259;
        let mut next = |count: u64| {
            // SplitMix64, rejection sampling avoids modulo bias.
            loop {
                rng = rng.wrapping_add(0x9e3779b97f4a7c15);
                let mut z = rng;
                z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
                z ^= z >> 31;
                let threshold = count.wrapping_neg() % count;
                if z >= threshold {
                    break z % count;
                }
            }
        };
        let propose_ms = 500 + 100 * next(5);
        let sensor_delay_ms = if next(2) == 0 { 0 } else { 300 };
        let gap_start = 100 * (3 + next(10));
        let gap_end = gap_start + 100 * (5 + next(21));
        let revoke = 100 * (3 + next(13));
        let initial_pressure = if no_invalidation {
            (next(41) as f64 - 20.0) / 1000.0
        } else {
            next(61) as f64 / 100.0
        };
        let initial_door = if no_invalidation {
            0.0
        } else {
            next(31) as f64 / 100.0 + 0.70
        };
        let permission_source = next(2) == 0;
        Self {
            generator_seed: seed,
            no_invalidation,
            propose_ms,
            initial_pressure,
            initial_door,
            sensor_delay_ms,
            pressure_gap_start_ms: if no_invalidation || permission_source {
                0
            } else {
                gap_start
            },
            pressure_gap_end_ms: if no_invalidation || permission_source {
                0
            } else {
                gap_end
            },
            revoke_ms: if no_invalidation || !permission_source {
                None
            } else {
                Some(revoke)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draws_cover_delivery_phases_and_replay_parameters_not_only_noise() {
        let mut phases = std::collections::BTreeSet::new();
        let mut pressure = std::collections::BTreeSet::new();
        for seed in 0..300 {
            let v = BuildingVariant::draw(seed, false);
            assert_eq!(v, BuildingVariant::draw(seed, false));
            phases.insert((v.sensor_delay_ms + 500 - v.propose_ms % 500) % 500);
            pressure.insert((v.initial_pressure * 100.0).round() as u64);
            let control = BuildingVariant::draw(seed, true);
            assert_eq!(control.revoke_ms, None);
            assert_eq!(control.pressure_gap_start_ms, control.pressure_gap_end_ms);
        }
        assert_eq!(phases.len(), 5);
        assert!(pressure.len() > 40);
    }
}
