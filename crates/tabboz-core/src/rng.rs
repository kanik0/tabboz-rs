// Deterministic RNG porting the semantics of the original random():
//
//   int random(int n) { return rand() % n; }   (Borland/MS semantics)
//
// The exact libc rand() sequence differs between the original Win32 build
// (Borland RTL), the web build (musl) and this port; what the game relies
// on is only the uniform 0..n-1 distribution, which is preserved exactly.
// This implementation uses the classic MSVC linear congruential generator,
// which keeps whole-game runs reproducible from a fixed seed (tests).

pub struct Rng {
    seed: u32,
}

impl Rng {
    pub fn with_seed(seed: u32) -> Self {
        Rng { seed }
    }

    /// Seeds from the wall clock (the original calls randomize() at startup).
    pub fn randomize() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Rng::with_seed(now as u32)
    }

    /// MSVC rand(): state = state * 214013 + 2531011, returns (state >> 16) & 0x7FFF.
    pub fn rand(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(214013).wrapping_add(2531011);
        (self.seed >> 16) & 0x7FFF
    }

    /// Borland random(n): rand() % n. Returns 0 for n <= 0 (the original
    /// would crash on n == 0; the game never calls it that way in practice).
    pub fn random(&mut self, n: i32) -> i32 {
        if n <= 0 {
            return 0;
        }
        (self.rand() % n as u32) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_from_fixed_seed() {
        let mut a = Rng::with_seed(42);
        let mut b = Rng::with_seed(42);
        for _ in 0..100 {
            assert_eq!(a.rand(), b.rand());
        }
    }

    #[test]
    fn rand_stays_in_15_bit_range() {
        let mut r = Rng::with_seed(1);
        for _ in 0..10_000 {
            assert!(r.rand() <= 0x7FFF);
        }
    }

    #[test]
    fn random_is_uniform_modulo() {
        let mut r = Rng::with_seed(7);
        let mut seen = [0usize; 5];
        for _ in 0..50_000 {
            seen[r.random(5) as usize] += 1;
        }
        // Every residue must appear roughly equally (10k +/- 10%).
        for c in seen {
            assert!(c > 9_000 && c < 11_000, "distribution skewed: {seen:?}");
        }
        assert_eq!(r.random(0), 0);
        assert_eq!(r.random(-3), 0);
    }
}
