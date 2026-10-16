/// Finite closed interval. Every arithmetic operation rounds outwards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    lo: f64,
    hi: f64,
}
impl Interval {
    pub fn new(lo: f64, hi: f64) -> Result<Self, String> {
        if !lo.is_finite() || !hi.is_finite() || lo > hi {
            return Err("interval must have finite ordered endpoints".into());
        }
        Ok(Self { lo, hi })
    }
    pub fn point(x: f64) -> Result<Self, String> {
        Self::new(x, x)
    }
    pub fn lo(self) -> f64 {
        self.lo
    }
    pub fn hi(self) -> f64 {
        self.hi
    }
    pub fn width(self) -> f64 {
        self.hi - self.lo
    }
    pub fn contains(self, x: f64) -> bool {
        x >= self.lo && x <= self.hi
    }
    pub fn subset_of(self, other: Self) -> bool {
        self.lo >= other.lo && self.hi <= other.hi
    }
    pub fn midpoint(self) -> f64 {
        self.lo + (self.hi - self.lo) / 2.0
    }
    pub fn intersect(self, other: Self) -> Option<Self> {
        Self::new(self.lo.max(other.lo), self.hi.min(other.hi)).ok()
    }
    pub fn hull(self, other: Self) -> Self {
        Self {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }
    pub(crate) fn add(self, other: Self) -> Self {
        Self {
            lo: (self.lo + other.lo).next_down(),
            hi: (self.hi + other.hi).next_up(),
        }
    }
    pub(crate) fn sub(self, other: Self) -> Self {
        self.add(Self {
            lo: -other.hi,
            hi: -other.lo,
        })
    }
    pub(crate) fn mul(self, other: Self) -> Self {
        let values = [
            self.lo * other.lo,
            self.lo * other.hi,
            self.hi * other.lo,
            self.hi * other.hi,
        ];
        Self {
            lo: values.into_iter().fold(f64::INFINITY, f64::min).next_down(),
            hi: values
                .into_iter()
                .fold(f64::NEG_INFINITY, f64::max)
                .next_up(),
        }
    }
    pub(crate) fn clip(self, lo: f64, hi: f64) -> Self {
        Self {
            lo: self.lo.clamp(lo, hi),
            hi: self.hi.clamp(lo, hi),
        }
    }
    pub(crate) fn around(value: f64, error: f64) -> Self {
        Self {
            lo: (value - error).next_down(),
            hi: (value + error).next_up(),
        }
    }
}
pub(crate) fn p(x: f64) -> Interval {
    Interval::point(x).expect("validated constant")
}
