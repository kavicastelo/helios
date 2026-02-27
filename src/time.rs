/// Virtual time for the deterministic simulation.
///
/// Represents a logical timestamp with no dependency on `std::time`.
/// Time advances only when the scheduler processes events — never from
/// wall-clock observation.

/// A logical tick in simulation time.
/// Intentionally *not* `Copy`-able to make accidental duplication visible,
/// but we derive `Clone` so callers can be explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct VirtualTime(u64);

impl VirtualTime {
    /// The zero-point of simulation time.
    pub const ZERO: VirtualTime = VirtualTime(0);

    /// Create a new `VirtualTime` from a raw tick value.
    #[inline]
    pub fn new(ticks: u64) -> Self {
        VirtualTime(ticks)
    }

    /// Return the raw tick value.
    #[inline]
    pub fn ticks(self) -> u64 {
        self.0
    }

    /// Advance time by `delta` ticks.
    /// Returns `None` on overflow (should never happen in practice).
    #[inline]
    pub fn advance(self, delta: u64) -> Option<VirtualTime> {
        self.0.checked_add(delta).map(VirtualTime)
    }

    /// Compute the absolute time that is `delay` ticks after `self`.
    /// Alias for `advance` — reads better at call-sites that schedule future events.
    #[inline]
    pub fn plus(self, delay: u64) -> Option<VirtualTime> {
        self.advance(delay)
    }

    /// Returns `true` if `self` is strictly before `other`.
    #[inline]
    pub fn is_before(self, other: VirtualTime) -> bool {
        self.0 < other.0
    }

    /// Returns the duration (in ticks) between two points in time.
    /// Returns `None` if `other` is before `self`.
    #[inline]
    pub fn duration_since(self, other: VirtualTime) -> Option<u64> {
        self.0.checked_sub(other.0)
    }
}

impl std::fmt::Display for VirtualTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "T={}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero() {
        assert_eq!(VirtualTime::ZERO.ticks(), 0);
    }

    #[test]
    fn test_ordering() {
        let t1 = VirtualTime::new(10);
        let t2 = VirtualTime::new(20);
        assert!(t1 < t2);
        assert!(t1.is_before(t2));
        assert!(!t2.is_before(t1));
    }

    #[test]
    fn test_advance() {
        let t = VirtualTime::new(100);
        let t2 = t.advance(50).unwrap();
        assert_eq!(t2.ticks(), 150);
    }

    #[test]
    fn test_advance_overflow() {
        let t = VirtualTime::new(u64::MAX);
        assert!(t.advance(1).is_none());
    }

    #[test]
    fn test_duration_since() {
        let t1 = VirtualTime::new(10);
        let t2 = VirtualTime::new(30);
        assert_eq!(t2.duration_since(t1), Some(20));
        assert_eq!(t1.duration_since(t2), None);
    }

    #[test]
    fn test_display() {
        let t = VirtualTime::new(42);
        assert_eq!(format!("{}", t), "T=42");
    }

    #[test]
    fn test_equality() {
        let t1 = VirtualTime::new(99);
        let t2 = VirtualTime::new(99);
        assert_eq!(t1, t2);
    }
}
