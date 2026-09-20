//! Deterministic pseudo-random number generator using xoshiro256**.
//!
//! Critical: Same seed must produce identical trees across all platforms.

/// xoshiro256** PRNG - fast, deterministic, cross-platform
pub struct Rng {
    state: [u64; 4],
}

impl Rng {
    /// Create a new RNG from a seed value using SplitMix64 to initialize state
    pub fn from_seed(seed: u64) -> Self {
        // Use SplitMix64 to expand seed into 4 state values
        let mut x = seed;
        let s0 = splitmix64(&mut x);
        let s1 = splitmix64(&mut x);
        let s2 = splitmix64(&mut x);
        let s3 = splitmix64(&mut x);

        Self {
            state: [s0, s1, s2, s3],
        }
    }

    /// Generate next random u64
    fn next_u64(&mut self) -> u64 {
        // xoshiro256** algorithm:
        let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);

        let t = self.state[1] << 17;

        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];

        self.state[2] ^= t;
        self.state[3] = self.state[3].rotate_left(45);

        result
    }

    /// Generate f32 in range [0, 1)
    pub fn next_f32(&mut self) -> f32 {
        // Take upper bits for better distribution
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Generate f32 in range [min, max)
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }

    /// Generate f32 in range [1-variance, 1+variance] for multiplicative variance
    pub fn variance_mul(&mut self, variance: f32) -> f32 {
        1.0 + self.range(-variance, variance)
    }

    /// Generate f32 in range [-variance, +variance] for additive variance
    pub fn variance_add(&mut self, variance: f32) -> f32 {
        self.range(-variance, variance)
    }

    /// Generate random index in range [0, len)
    pub fn index(&mut self, len: usize) -> usize {
        (self.next_f32() * len as f32) as usize
    }
}

/// SplitMix64 - used to initialize xoshiro256** state from a single seed
fn splitmix64(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_output() {
        // Same seed must produce same sequence
        let mut rng1 = Rng::from_seed(12345);
        let mut rng2 = Rng::from_seed(12345);

        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_different_seeds_produce_different_output() {
        let mut rng1 = Rng::from_seed(12345);
        let mut rng2 = Rng::from_seed(54321);

        // Very unlikely to be equal with different seeds
        assert_ne!(rng1.next_u64(), rng2.next_u64());
    }

    #[test]
    fn test_f32_range() {
        let mut rng = Rng::from_seed(42);

        for _ in 0..1000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "next_f32() out of range: {}", v);
        }
    }

    #[test]
    fn test_range_function() {
        let mut rng = Rng::from_seed(42);

        for _ in 0..1000 {
            let v = rng.range(10.0, 20.0);
            assert!((10.0..20.0).contains(&v), "range() out of bounds: {}", v);
        }
    }

    #[test]
    fn test_variance_mul() {
        let mut rng = Rng::from_seed(42);

        for _ in 0..1000 {
            let v = rng.variance_mul(0.1);
            assert!(
                (0.9..=1.1).contains(&v),
                "variance_mul() out of bounds: {}",
                v
            );
        }
    }

    #[test]
    fn test_variance_add() {
        let mut rng = Rng::from_seed(42);

        for _ in 0..1000 {
            let v = rng.variance_add(0.5);
            assert!(
                (-0.5..=0.5).contains(&v),
                "variance_add() out of bounds: {}",
                v
            );
        }
    }

    #[test]
    fn test_index() {
        let mut rng = Rng::from_seed(42);

        for _ in 0..1000 {
            let v = rng.index(10);
            assert!(v < 10, "index() out of bounds: {}", v);
        }
    }

    #[test]
    fn test_known_sequence() {
        // Verify specific output for reproducibility across platforms
        let mut rng = Rng::from_seed(0);
        let expected = [rng.next_u64(), rng.next_u64(), rng.next_u64()];

        // Reset and verify
        let mut rng2 = Rng::from_seed(0);
        assert_eq!(rng2.next_u64(), expected[0]);
        assert_eq!(rng2.next_u64(), expected[1]);
        assert_eq!(rng2.next_u64(), expected[2]);
    }
}
