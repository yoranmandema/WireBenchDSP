//! Small math helpers. Per-sample code avoids `libm` where a polynomial
//! approximation is cheaper; coefficient design uses `libm` for accuracy.

pub use core::f32::consts::PI;

pub const TAU: f32 = 2.0 * PI;

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    libm::powf(10.0, db * 0.05)
}

#[inline]
pub fn lin_to_db(x: f32) -> f32 {
    20.0 * libm::log10f(x.abs().max(1e-9))
}

#[inline]
pub fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[inline]
pub fn clamp01(x: f32) -> f32 {
    clamp(x, 0.0, 1.0)
}

#[inline]
pub fn clamp_u32(x: u32, lo: u32, hi: u32) -> u32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
pub fn wrap01(x: f32) -> f32 {
    x - libm::floorf(x)
}

/// Blend `dry` toward `wet` by `amount` (0 = dry, 1 = wet).
#[inline]
pub fn mix(dry: f32, wet: f32, amount: f32) -> f32 {
    dry + (wet - dry) * amount
}

/// Polynomial tanh approximation (Padé [3/3]-ish), clamped outside |x| > 4.
/// Max error ~2% around |x| = 1, which is inaudible inside a distortion stage.
#[inline]
pub fn tanh_approx(x: f32) -> f32 {
    if x > 4.0 {
        1.0
    } else if x < -4.0 {
        -1.0
    } else {
        let x2 = x * x;
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    }
}

/// Clamp to the [-1, 1] range without branching on NaN.
#[inline]
pub fn saturate(x: f32) -> f32 {
    clamp(x, -1.0, 1.0)
}

/// Convert a frequency in Hz to radians per sample at `fs`.
#[inline]
pub fn hz_to_rad(f: f32, fs: f32) -> f32 {
    TAU * f / fs
}

/// One-pole smoothing coefficient from a time constant in milliseconds.
/// `coef = 1 - exp(-1 / (tau * fs))`
#[inline]
pub fn smoothing_coef_ms(tau_ms: f32, fs: f32) -> f32 {
    let samples = (tau_ms * 0.001) * fs;
    1.0 - libm::expf(-1.0 / samples.max(1.0))
}

/// Map a 12-bit ADC reading (0..4095) to 0..1.
#[inline]
pub fn adc12_to_norm(v: u16) -> f32 {
    (v as f32 / 4095.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_roundtrip() {
        for db in [-60.0, -24.0, -6.0, 0.0, 6.0, 24.0] {
            let lin = db_to_lin(db);
            assert!((lin_to_db(lin) - db).abs() < 1e-4, "db roundtrip {db}");
        }
    }

    #[test]
    fn tanh_approximation_is_close() {
        for i in -40..=40 {
            let x = i as f32 / 10.0;
            let approx = tanh_approx(x);
            let exact = libm::tanhf(x);
            let err = (approx - exact).abs();
            assert!(err < 0.03, "tanh err {err} at {x}");
        }
    }

    #[test]
    fn clamp_and_lerp() {
        assert_eq!(clamp(2.0, 0.0, 1.0), 1.0);
        assert_eq!(clamp(-2.0, 0.0, 1.0), 0.0);
        assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
        assert_eq!(mix(0.0, 10.0, 0.25), 2.5);
    }

    #[test]
    fn adc_mapping() {
        assert_eq!(adc12_to_norm(0), 0.0);
        assert_eq!(adc12_to_norm(4095), 1.0);
        assert!((adc12_to_norm(2048) - 0.5).abs() < 0.001);
    }
}
