//! Random numbers.
//!
//! `PyRng` is the Mersenne Twister exactly as Python's `random.Random` uses
//! it. The city was first laid out by a Python prototype, and its roofs are
//! a function of the order and the values of these calls: keeping them
//! bit-identical keeps every roof where it has always been.
//!
//! `Fast` is a small xorshift generator for the things that only need to
//! look random (where a bird flies, when a star falls).

pub struct PyRng {
    mt: [u32; 624],
    at: usize,
}

impl PyRng {
    pub fn new(seed: u64) -> Self {
        let mut r = PyRng { mt: [0; 624], at: 624 };
        let key: Vec<u32> = if seed >> 32 == 0 { vec![seed as u32] } else { vec![seed as u32, (seed >> 32) as u32] };
        r.init(19650218);
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..624.max(key.len()) {
            r.mt[i] = (r.mt[i] ^ (r.mt[i - 1] ^ (r.mt[i - 1] >> 30)).wrapping_mul(1664525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= 624 {
                r.mt[0] = r.mt[623];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..623 {
            r.mt[i] = (r.mt[i] ^ (r.mt[i - 1] ^ (r.mt[i - 1] >> 30)).wrapping_mul(1566083941)).wrapping_sub(i as u32);
            i += 1;
            if i >= 624 {
                r.mt[0] = r.mt[623];
                i = 1;
            }
        }
        r.mt[0] = 0x8000_0000;
        r
    }

    fn init(&mut self, s: u32) {
        self.mt[0] = s;
        for i in 1..624 {
            self.mt[i] = 1812433253u32.wrapping_mul(self.mt[i - 1] ^ (self.mt[i - 1] >> 30)).wrapping_add(i as u32);
        }
        self.at = 624;
    }

    fn next(&mut self) -> u32 {
        if self.at >= 624 {
            for k in 0..624 {
                let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % 624] & 0x7fff_ffff);
                self.mt[k] = self.mt[(k + 397) % 624] ^ (y >> 1) ^ if y & 1 != 0 { 0x9908_b0df } else { 0 };
            }
            self.at = 0;
        }
        let mut y = self.mt[self.at];
        self.at += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// A float in [0, 1), 53 bits of it.
    pub fn random(&mut self) -> f64 {
        let a = (self.next() >> 5) as f64;
        let b = (self.next() >> 6) as f64;
        (a * 67108864.0 + b) / 9007199254740992.0
    }

    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.random()
    }

    /// An integer in [0, n).
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let bits = 32 - n.leading_zeros();
        loop {
            let r = self.next() >> (32 - bits);
            if r < n {
                return r;
            }
        }
    }

    /// An integer in [a, b], both ends included.
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        a + self.below((b - a + 1) as u32) as i64
    }

    pub fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[self.below(of.len() as u32) as usize]
    }
}

#[derive(Clone)]
pub struct Fast(u64);

impl Fast {
    pub fn new(seed: u64) -> Self {
        let mut r = Fast(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03);
        for _ in 0..4 {
            r.u64();
        }
        r
    }

    pub fn u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A float in [0, 1).
    pub fn f(&mut self) -> f32 {
        (self.u64() >> 40) as f32 / 16777216.0
    }

    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }

    pub fn below(&mut self, n: usize) -> usize {
        ((self.u64() >> 33) as usize) % n.max(1)
    }

    pub fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[self.below(of.len())]
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
}

/// A fixed scramble of two integers into [0, 1): the same grain every frame.
#[inline]
pub fn hash2(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / 16777216.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The values Python's `random.Random` gives for the same seeds and calls.
    #[test]
    fn matches_python() {
        let mut r = PyRng::new(12);
        assert_eq!(r.random(), 0.4745706786885481);
        assert_eq!(r.random(), 0.6574725026572553);
        assert_eq!(r.randint(0, 230), 170);
        assert_eq!(r.uniform(-1.0, 1.0), -0.30042925264536824);
        assert_eq!(r.pick(&[10, 20, 30, 40]), 40);
        let mut r = PyRng::new(95045);
        assert_eq!(r.random(), 0.07815630348162117);
        assert_eq!(r.below(4), 3);
        assert_eq!(PyRng::new((1 << 40) + 7).random(), 0.6137037779936511);
    }
}
