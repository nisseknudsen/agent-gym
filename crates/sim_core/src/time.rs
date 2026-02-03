/// Q32.32 fixed-point time duration in microseconds.
///
/// Storage: `u64` with 32 integer bits + 32 fractional bits.
/// Base unit: microseconds (1 second = 1,000,000 us).
/// Range: 0 to ~4294 seconds with sub-microsecond precision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Micros(u64);

impl Micros {
    const FRAC_BITS: u32 = 32;
    const MICROS_PER_SEC: u64 = 1_000_000;

    /// Create from whole seconds.
    pub const fn from_secs(secs: u32) -> Self {
        Self((secs as u64 * Self::MICROS_PER_SEC) << Self::FRAC_BITS)
    }

    /// Create from whole milliseconds.
    pub const fn from_millis(millis: u32) -> Self {
        Self((millis as u64 * 1_000) << Self::FRAC_BITS)
    }

    /// Create from whole microseconds.
    pub const fn from_micros(micros: u32) -> Self {
        Self((micros as u64) << Self::FRAC_BITS)
    }

    /// Convert to tick count at the given tick rate.
    ///
    /// Formula: ticks = (micros * tick_hz) / MICROS_PER_SEC
    /// This uses 128-bit intermediate to avoid overflow.
    pub const fn to_ticks(self, tick_hz: u32) -> u64 {
        // self.0 is Q32.32 microseconds
        // We want: (self.0 >> 32) * tick_hz / MICROS_PER_SEC
        // But we need to preserve fractional precision, so:
        // ticks = (self.0 * tick_hz) / (MICROS_PER_SEC << 32)
        let numer = self.0 as u128 * tick_hz as u128;
        let denom = Self::MICROS_PER_SEC << Self::FRAC_BITS;
        (numer / denom as u128) as u64
    }

    /// Returns the raw Q32.32 value.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl core::ops::Add for Micros {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl core::ops::Sub for Micros {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl core::ops::Mul<u32> for Micros {
    type Output = Self;
    fn mul(self, rhs: u32) -> Self {
        Self(self.0 * rhs as u64)
    }
}

impl core::ops::Div<u32> for Micros {
    type Output = Self;
    fn div(self, rhs: u32) -> Self {
        Self(self.0 / rhs as u64)
    }
}

/// Q32.32 fixed-point speed in cells per second.
///
/// Storage: `u64` with 32 integer bits + 32 fractional bits.
/// Separate type for type safety (can't accidentally mix duration and speed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Speed(u64);

impl Speed {
    const FRAC_BITS: u32 = 32;

    /// Create from whole cells per second.
    pub const fn from_cells_per_sec(cps: u32) -> Self {
        Self((cps as u64) << Self::FRAC_BITS)
    }

    /// Create from a fractional cells per second (numer/denom).
    pub const fn from_cells_per_sec_frac(numer: u32, denom: u32) -> Self {
        // (numer / denom) in Q32.32 = (numer << 32) / denom
        Self(((numer as u64) << Self::FRAC_BITS) / denom as u64)
    }

    /// Convert speed to tick interval (ticks between moves).
    ///
    /// Formula: interval = tick_hz / speed
    /// Returns ticks between each cell movement.
    pub const fn to_tick_interval(self, tick_hz: u32) -> u64 {
        if self.0 == 0 {
            return u64::MAX;
        }
        // self.0 is Q32.32 cells/sec
        // We want: tick_hz / (self.0 >> 32)
        // But to preserve precision: (tick_hz << 32) / self.0
        ((tick_hz as u64) << Self::FRAC_BITS) / self.0
    }

    /// Returns the raw Q32.32 value.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micros_from_secs() {
        let m = Micros::from_secs(1);
        // 1 second = 1_000_000 us in Q32.32
        assert_eq!(m.0, 1_000_000 << 32);
    }

    #[test]
    fn micros_from_millis() {
        let m = Micros::from_millis(500);
        // 500 ms = 500_000 us in Q32.32
        assert_eq!(m.0, 500_000 << 32);
    }

    #[test]
    fn micros_to_ticks() {
        // 1 second at 60 Hz = 60 ticks
        let m = Micros::from_secs(1);
        assert_eq!(m.to_ticks(60), 60);

        // 30 seconds at 60 Hz = 1800 ticks
        let m = Micros::from_secs(30);
        assert_eq!(m.to_ticks(60), 1800);

        // 500 ms at 60 Hz = 30 ticks
        let m = Micros::from_millis(500);
        assert_eq!(m.to_ticks(60), 30);
    }

    #[test]
    fn speed_to_tick_interval() {
        // 2 cells/sec at 60 Hz = 30 ticks between moves
        let s = Speed::from_cells_per_sec(2);
        assert_eq!(s.to_tick_interval(60), 30);

        // 1 cell/sec at 60 Hz = 60 ticks between moves
        let s = Speed::from_cells_per_sec(1);
        assert_eq!(s.to_tick_interval(60), 60);

        // 0.5 cells/sec at 60 Hz = 120 ticks between moves
        let s = Speed::from_cells_per_sec_frac(1, 2);
        assert_eq!(s.to_tick_interval(60), 120);
    }

    #[test]
    fn speed_zero() {
        let s = Speed::from_cells_per_sec(0);
        assert_eq!(s.to_tick_interval(60), u64::MAX);
    }

    #[test]
    fn micros_arithmetic() {
        let a = Micros::from_secs(5);
        let b = Micros::from_secs(3);

        assert_eq!((a + b).to_ticks(60), 480); // 8 seconds
        assert_eq!((a - b).to_ticks(60), 120); // 2 seconds
        assert_eq!((a * 2).to_ticks(60), 600); // 10 seconds
        assert_eq!((a / 5).to_ticks(60), 60); // 1 second
    }
}
