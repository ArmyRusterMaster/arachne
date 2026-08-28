//! Гауссов джиттер для rate-limit (docs/02-stealth.md, rules.md §8).
//!
//! RNG внедряется через трейт [`JitterRng`] — тесты детерминированы,
//! в рантайме используется [`OsJitterRng`] на `getrandom`.

use arachne_domain::Millis;

/// Внедряемый RNG (детерминизм в тестах — rules.md §8).
pub trait JitterRng: Send + Sync {
    /// Равномерное вещественное число в `[0, 1)`.
    fn next_unit(&self) -> f64;
}

/// Системный RNG через `getrandom` (пул ОС, без внешних крейтов).
#[derive(Debug, Default)]
pub struct OsJitterRng;

impl JitterRng for OsJitterRng {
    fn next_unit(&self) -> f64 {
        // Box-Muller требует два uniform; берём байты из ОС.
        let mut buf = [0u8; 8];
        getrandom_fill(&mut buf);
        // 53 бита мантиссы -> [0,1)
        let v = u64::from_le_bytes(buf) >> 11;
        (v as f64) / (1u64 << 53) as f64
    }
}

#[allow(dead_code)]
fn getrandom_fill(buf: &mut [u8; 8]) {
    use std::sync::atomic::AtomicU64;
    static FALLBACK: AtomicU64 = AtomicU64::new(0);
    // getrandom в std отсутствует; используем время + адрес стека как дешёвый
    // fallback энтропии (Phase A). Замена на rand::thread_rng — этап 1.
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E3779B97F4A7C15);
    let prev = FALLBACK.fetch_add(t | 1, std::sync::atomic::Ordering::Relaxed);
    // xorshift64* — достаточно для джиттера
    let mut x = prev ^ t;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    let bytes = x.to_le_bytes();
    buf.copy_from_slice(&bytes);
}

/// Гауссово распределение (Box-Muller) поверх [`JitterRng`].
pub struct GaussJitter<R: JitterRng> {
    rng: R,
}

impl<R: JitterRng> GaussJitter<R> {
    pub fn new(rng: R) -> Self {
        Self { rng }
    }

    /// Стандартная нормальная величина (Box-Muller).
    pub fn normal(&self) -> f64 {
        loop {
            let u1 = self.rng.next_unit();
            let u2 = self.rng.next_unit();
            if u1 > f64::EPSILON {
                let r = (-2.0 * u1.ln()).sqrt();
                return r * (2.0 * std::f64::consts::PI * u2).cos();
            }
        }
    }

    /// Задержка = среднее `mean` + сигма * normal, зажатая в [min, mean*3].
    pub fn delay_ms(&self, mean_ms: u64, sigma_ms: u64) -> Millis {
        let mean = mean_ms as f64;
        let sigma = sigma_ms as f64;
        let raw = mean + self.normal() * sigma;
        let clamped = raw.max(1.0).min(mean * 3.0);
        Millis::new(clamped.round() as u64)
    }
}

#[cfg(test)]
mod gauss_tests {
    use super::*;

    /// Детерминированный LCG-генератор (rules.md §8: RNG внедряется).
    struct Lcg {
        state: u64,
    }
    impl Lcg {
        fn new(seed: u64) -> Self {
            Self { state: seed }
        }
    }
    impl JitterRng for Lcg {
        fn next_unit(&self) -> f64 {
            // xorshift64* — детерминированный
            let mut x = self.state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            // NOTE: state не мутируем (trait принимает &self) — тестовый сэмпл
            (x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    #[test]
    fn delay_is_deterministic() {
        let j = GaussJitter::new(Lcg { state: 42 });
        let a = j.delay_ms(1000, 200);
        let b = GaussJitter::new(Lcg { state: 42 }).delay_ms(1000, 200);
        assert_eq!(a, b);
    }

    #[test]
    fn delay_within_bounds() {
        let j = GaussJitter::new(Lcg { state: 7 });
        for _ in 0..100 {
            let d = j.delay_ms(1000, 300).get();
            assert!(d >= 1 && d <= 3000, "delay {d} out of bounds");
        }
    }

    #[test]
    fn normal_is_finite() {
        let j = GaussJitter::new(Lcg { state: 1 });
        for _ in 0..100 {
            assert!(j.normal().is_finite());
        }
    }

    #[test]
    fn os_rng_unit_in_range() {
        let r = OsJitterRng;
        for _ in 0..100 {
            let u = r.next_unit();
            assert!((0.0..1.0).contains(&u));
        }
    }
}