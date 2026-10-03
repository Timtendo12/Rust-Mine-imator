//! The random numbers of the original (`random`, `random_range`, `irandom`,
//! `random_set_seed`): a Mersenne Twister read through a uniform
//! distribution over 0..1, as its C++ runtime does (`std::mt19937`,
//! `std::uniform_real_distribution<double>`), so that spawners with a seed
//! of their own behave alike.

const N: usize = 624;
const M: usize = 397;

/// MT19937.
#[derive(Clone)]
pub struct Rng {
    state: [u32; N],
    index: usize,
}

impl std::fmt::Debug for Rng {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rng").field("index", &self.index).finish_non_exhaustive()
    }
}

impl Rng {
    /// `random_set_seed`: the seed is cut to a whole number.
    pub fn new(seed: f64) -> Self {
        Self::from_u32(seed as i64 as u32)
    }

    pub fn from_u32(seed: u32) -> Self {
        let mut state = [0u32; N];
        state[0] = seed;
        for i in 1..N {
            state[i] = 1_812_433_253u32.wrapping_mul(state[i - 1] ^ (state[i - 1] >> 30)).wrapping_add(i as u32);
        }
        Self { state, index: N }
    }

    fn twist(&mut self) {
        for i in 0..N {
            let y = (self.state[i] & 0x8000_0000) | (self.state[(i + 1) % N] & 0x7fff_ffff);
            let mut next = self.state[(i + M) % N] ^ (y >> 1);
            if y & 1 != 0 {
                next ^= 0x9908_b0df;
            }
            self.state[i] = next;
        }
        self.index = 0;
    }

    pub fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// A number in 0..1 from two draws, the first being the low half
    /// (`std::generate_canonical<double, 53>`).
    fn unit(&mut self) -> f64 {
        let low = self.next_u32() as f64;
        let high = self.next_u32() as f64;
        let value = (low + high * 4_294_967_296.0) / 18_446_744_073_709_551_616.0;
        // Rounding can reach 1, which the distribution never returns.
        value.min(1.0 - f64::EPSILON / 2.0)
    }

    /// `random(max)`: 0 up to, not including, `max`.
    pub fn random(&mut self, max: f64) -> f64 {
        self.unit() * max
    }

    /// `random_range(min, max)`
    pub fn range(&mut self, min: f64, max: f64) -> f64 {
        min + self.random(max - min)
    }

    /// `irandom(max)`: a whole number from 0 to `max`, both included.
    pub fn irandom(&mut self, max: i64) -> i64 {
        self.random((max + 1) as f64) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_the_reference_mersenne_twister() {
        // The 10000th output of mt19937 with the default seed is fixed by
        // the C++ standard.
        let mut rng = Rng::from_u32(5489);
        let mut last = 0;
        for _ in 0..10_000 {
            last = rng.next_u32();
        }
        assert_eq!(last, 4_123_659_995);
        // First outputs for seed 1, from the reference implementation.
        let mut rng = Rng::from_u32(1);
        assert_eq!([rng.next_u32(), rng.next_u32()], [1_791_095_845, 4_282_876_139]);
    }

    #[test]
    fn numbers_stay_in_range_and_repeat_per_seed() {
        let mut a = Rng::new(42.9);
        let mut b = Rng::new(42.0);
        for _ in 0..1000 {
            let x = a.random(10.0);
            assert_eq!(x, b.random(10.0));
            assert!((0.0..10.0).contains(&x));
        }
        let mut rng = Rng::new(7.0);
        let mut seen = [false; 4];
        for _ in 0..200 {
            let n = rng.irandom(3);
            assert!((0..=3).contains(&n));
            seen[n as usize] = true;
            let r = rng.range(-5.0, -2.0);
            assert!((-5.0..-2.0).contains(&r));
        }
        assert_eq!(seen, [true; 4]);
        // The first number for seed 1: two draws, the low half first.
        let mut rng = Rng::from_u32(1);
        let expected = (1_791_095_845.0 + 4_282_876_139.0 * 4_294_967_296.0) / 18_446_744_073_709_551_616.0;
        assert_eq!(rng.random(1.0), expected);
    }
}
