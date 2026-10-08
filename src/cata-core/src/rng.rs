//! Port of the C++ random-number helpers in `src/cpp/rng.{h,cpp}`.
//!
//! This module reproduces two things exactly, because they are fully specified
//! and platform-independent:
//!
//! * The engine `std::minstd_rand0` — a linear congruential generator with
//!   `a = 16807`, `c = 0`, `m = 2^31 - 1` (see [`MinstdRand0`]).
//! * The `djb2` string hash used for world seeding ([`djb2_hash`]).
//!
//! Crucially, `minstd_rand0` only ever yields values in `[1, m - 1]`, i.e. a
//! 31-bit range, **not** the full `u32` range. The distribution helpers below
//! map from that range, so they are unbiased.
//!
//! The higher-level helpers ([`Rng::range`], [`Rng::rng_float`], ...) are
//! deterministic and unbiased, but are **not yet bit-for-bit identical to
//! libstdc++'s `std::uniform_int_distribution` and friends**. Reaching exact
//! distribution parity is a prerequisite for the Phase 1 conformance harness and
//! is tracked as follow-up work; it cannot be claimed until it is checked
//! against the C++ binary, which requires a C++23 toolchain that is not
//! available in every environment.

/// The default engine, `std::minstd_rand0`.
///
/// The recurrence is `x_{n+1} = (16807 * x_n) mod (2^31 - 1)`. The state is
/// never zero (the seed is mapped to `1` if it would be), matching the standard
/// engine's non-zero range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinstdRand0 {
    state: u64,
}

impl MinstdRand0 {
    /// Multiplier `a`.
    pub const A: u64 = 16807;
    /// Modulus `m`.
    pub const M: u64 = 2_147_483_647;

    /// Construct the engine from a seed. A zero seed is mapped to `1`, matching
    /// the standard engine's valid state range `[1, m - 1]`.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let state = (u64::from(seed) % Self::M).max(1);
        Self { state }
    }

    /// Advance the engine and return the next raw output, in `[1, m - 1]`.
    pub fn next_u32(&mut self) -> u32 {
        self.state = (Self::A * self.state) % Self::M;
        u32::try_from(self.state).expect("minstd_rand0 state always fits in u32")
    }
}

/// The `djb2` string hash used for world seeding.
///
/// This matches the C++ `djb2_hash(const unsigned char *)`, which hashes a
/// NUL-terminated string. The returned value is reinterpreted as `i32`, exactly
/// as the C++ function returns `int` from an `unsigned int` accumulator.
#[must_use]
pub fn djb2_hash(input: &[u8]) -> i32 {
    let mut hash: u32 = 5381;
    for &c in input {
        // hash * 33 + c
        hash = hash
            .wrapping_shl(5)
            .wrapping_add(hash)
            .wrapping_add(u32::from(c));
    }
    hash as i32
}

/// A seeded random-number generator mirroring the C++ free functions.
#[derive(Clone, Debug)]
pub struct Rng {
    engine: MinstdRand0,
}

impl Rng {
    /// Number of distinct engine states, `m - 1`. The engine is uniform over
    /// `[0, STATE_SPAN)` after subtracting the engine's minimum of `1`.
    const STATE_SPAN: u64 = MinstdRand0::M - 1;

    /// Create a generator seeded with `seed`.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self {
            engine: MinstdRand0::new(seed),
        }
    }

    /// Raw next engine value, equivalent to `rng_get_engine()()` / `rng_bits()`.
    ///
    /// Note: the C++ `rng_bits()` widens the engine's output to the full `u32`
    /// range; this returns the raw 31-bit engine value instead.
    pub fn bits(&mut self) -> u32 {
        self.engine.next_u32()
    }

    /// Draw uniformly from `[0, STATE_SPAN^2)` by combining two engine states.
    fn raw_u64(&mut self) -> u64 {
        let a = u64::from(self.engine.next_u32()) - 1; // [0, STATE_SPAN)
        let b = u64::from(self.engine.next_u32()) - 1; // [0, STATE_SPAN)
        a * Self::STATE_SPAN + b
    }

    /// Uniform integer in `[lo, hi]`, equivalent to `rng(lo, hi)`.
    ///
    /// `lo`/`hi` are swapped when `lo > hi`, matching the C++ behaviour. A
    /// rejection step removes modulo bias.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        let (lo, hi) = if lo > hi { (hi, lo) } else { (lo, hi) };
        let span = u64::try_from(i64::from(hi) - i64::from(lo) + 1).expect("span is positive");

        let total = Self::STATE_SPAN * Self::STATE_SPAN;
        debug_assert!(span <= total);
        let zone = total - (total % span);
        loop {
            let v = self.raw_u64();
            if v < zone {
                let offset = i64::try_from(v % span).expect("offset fits in i64");
                return i32::try_from(i64::from(lo) + offset).expect("result stays within i32");
            }
        }
    }

    /// Uniform float in `[lo, hi)`, equivalent to `rng_float(lo, hi)`.
    pub fn rng_float(&mut self, lo: f64, hi: f64) -> f64 {
        let (lo, hi) = if lo > hi { (hi, lo) } else { (lo, hi) };
        let v = u64::from(self.engine.next_u32()) - 1; // [0, STATE_SPAN)
        let unit = v as f64 / Self::STATE_SPAN as f64; // [0, 1)
        lo + (hi - lo) * unit
    }

    /// `true` with probability `1 / chance`, equivalent to `one_in(chance)`.
    pub fn one_in(&mut self, chance: i32) -> bool {
        chance <= 1 || self.range(0, chance - 1) == 0
    }

    /// `true` with probability `x / y`, equivalent to `x_in_y(x, y)`.
    pub fn x_in_y(&mut self, x: f64, y: f64) -> bool {
        self.rng_float(0.0, 1.0) <= x / y
    }

    /// Sum of `number` rolls of a `sides`-sided die, equivalent to `dice()`.
    pub fn dice(&mut self, number: i32, sides: i32) -> i32 {
        (0..number).map(|_| self.range(1, sides)).sum()
    }

    /// Probabilistically round a double to an int, equivalent to `roll_remainder()`.
    pub fn roll_remainder(&mut self, value: f64) -> i32 {
        let integ = value.trunc();
        let frac = value - integ;
        if value > 0.0 && value > integ && self.x_in_y(frac, 1.0) {
            (integ + 1.0) as i32
        } else if value < 0.0 && value < integ && self.x_in_y(-frac, 1.0) {
            (integ - 1.0) as i32
        } else {
            integ as i32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{djb2_hash, MinstdRand0, Rng};

    /// `std::minstd_rand0` with the default seed `1` produces this well-known
    /// sequence; if our recurrence is wrong, these will not match.
    #[test]
    fn minstd_rand0_matches_canonical_sequence() {
        let mut eng = MinstdRand0::new(1);
        let expected: [u32; 10] = [
            16807,
            282_475_249,
            1_622_650_073,
            984_943_658,
            1_144_108_930,
            470_211_272,
            101_027_544,
            1_457_850_878,
            1_458_777_923,
            2_007_237_709,
        ];
        let got: Vec<u32> = (0..10).map(|_| eng.next_u32()).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn zero_seed_maps_to_one() {
        assert_eq!(
            MinstdRand0::new(1).next_u32(),
            MinstdRand0::new(0).next_u32()
        );
    }

    #[test]
    fn djb2_matches_reference_values() {
        // Empty input: the accumulator is returned unchanged.
        assert_eq!(djb2_hash(b""), 5381_i32);
        // "a" => 5381 * 33 + 97 = 177670.
        assert_eq!(djb2_hash(b"a"), 177_670_i32);
    }

    #[test]
    fn range_is_inclusive_and_handles_reversed_bounds() {
        let mut rng = Rng::new(12345);
        for _ in 0..10_000 {
            let v = rng.range(2, 5);
            assert!((2..=5).contains(&v));
        }
        // Swapped bounds must behave the same as sorted bounds.
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        assert_eq!(a.range(9, 3), b.range(3, 9));
    }

    #[test]
    fn rng_float_covers_full_unit_interval() {
        let mut rng = Rng::new(1);
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for _ in 0..100_000 {
            let v = rng.rng_float(0.0, 1.0);
            assert!((0.0..1.0).contains(&v));
            min = min.min(v);
            max = max.max(v);
        }
        // A correct mapping reaches both ends of the interval.
        assert!(min < 0.01, "min = {min}");
        assert!(max > 0.99, "max = {max}");
    }

    #[test]
    fn one_in_certain_cases() {
        let mut rng = Rng::new(1);
        assert!(rng.one_in(1));
        assert!(rng.one_in(0));
    }

    #[test]
    fn x_in_y_is_roughly_fair() {
        let mut rng = Rng::new(1);
        let mut hits = 0;
        for _ in 0..10_000 {
            if rng.x_in_y(1.0, 2.0) {
                hits += 1;
            }
        }
        // Expect roughly half; allow a generous band.
        assert!((4_000..=6_000).contains(&hits), "hits = {hits}");
    }

    #[test]
    fn dice_and_roll_remainder_bounds() {
        let mut rng = Rng::new(99);
        for _ in 0..1_000 {
            let v = rng.dice(3, 6);
            assert!((3..=18).contains(&v));
        }
        // roll_remainder is exact for integral inputs.
        let mut rng = Rng::new(1);
        assert_eq!(rng.roll_remainder(5.0), 5);
        assert_eq!(rng.roll_remainder(-5.0), -5);
    }
}
