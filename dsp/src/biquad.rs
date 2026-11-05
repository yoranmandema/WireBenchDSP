//! Biquadratic IIR filter — the workhorse of pedal tone shaping.
//!
//! Implemented in **transposed direct form II** (2 state variables, good
//! numerical behaviour). Coefficients come from the RBJ audio EQ cookbook.
//!
//! A biquad is only stable if the poles are inside the unit circle; the
//! cookbook formulas guarantee that for `Q > 0` and `f0 < fs/2`.

use crate::util::{clamp, db_to_lin};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BiquadKind {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    AllPass,
    Peak,
    LowShelf,
    HighShelf,
}

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self::identity()
    }
}

impl Biquad {
    /// Unity gain, no filtering.
    pub const fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Build from raw coefficients. `a0` is normalised out for you.
    pub fn new(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        let inv = 1.0 / a0;
        Self {
            b0: b0 * inv,
            b1: b1 * inv,
            b2: b2 * inv,
            a1: a1 * inv,
            a2: a2 * inv,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Replace coefficients, keeping the existing state.
    pub fn set_coefficients(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        let inv = 1.0 / a0;
        self.b0 = b0 * inv;
        self.b1 = b1 * inv;
        self.b2 = b2 * inv;
        self.a1 = a1 * inv;
        self.a2 = a2 * inv;
    }

    /// Design a filter with the RBJ cookbook.
    ///
    /// - `fs`  — sample rate in Hz
    /// - `f0`  — centre/corner frequency in Hz
    /// - `q`   — Q factor (> 0)
    /// - `gain_db` — shelf/peak gain in dB (ignored by the other kinds)
    pub fn make(kind: BiquadKind, fs: f32, f0: f32, q: f32, gain_db: f32) -> Self {
        let f0 = clamp(f0, 1.0, fs * 0.499);
        let q = q.max(0.01);
        // A = 10^(dB/40); db_to_lin(db) is 10^(db/20), so halve the dB value.
        let a = db_to_lin(gain_db * 0.5);
        let w0 = crate::util::hz_to_rad(f0, fs);
        let cw = libm::cosf(w0);
        let sw = libm::sinf(w0);
        let alpha = sw / (2.0 * q);
        let sqrt_a = libm::sqrtf(a);

        let (b0, b1, b2, a0, a1, a2) = match kind {
            BiquadKind::LowPass => {
                let b0 = (1.0 - cw) * 0.5;
                let b1 = 1.0 - cw;
                let b2 = (1.0 - cw) * 0.5;
                (b0, b1, b2, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
            }
            BiquadKind::HighPass => {
                let b0 = (1.0 + cw) * 0.5;
                let b1 = -(1.0 + cw);
                let b2 = (1.0 + cw) * 0.5;
                (b0, b1, b2, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
            }
            BiquadKind::BandPass => {
                let b0 = alpha;
                let b1 = 0.0;
                let b2 = -alpha;
                (b0, b1, b2, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
            }
            BiquadKind::Notch => {
                let b0 = 1.0;
                let b1 = -2.0 * cw;
                let b2 = 1.0;
                (b0, b1, b2, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
            }
            BiquadKind::AllPass => {
                let b0 = 1.0 - alpha;
                let b1 = -2.0 * cw;
                let b2 = 1.0 + alpha;
                (b0, b1, b2, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
            }
            BiquadKind::Peak => {
                let b0 = 1.0 + alpha * a;
                let b1 = -2.0 * cw;
                let b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                (b0, b1, b2, a0, -2.0 * cw, 1.0 - alpha / a)
            }
            BiquadKind::LowShelf => {
                let b0 = a * ((a + 1.0) - (a - 1.0) * cw + 2.0 * sqrt_a * alpha);
                let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cw);
                let b2 = a * ((a + 1.0) - (a - 1.0) * cw - 2.0 * sqrt_a * alpha);
                let a0 = (a + 1.0) + (a - 1.0) * cw + 2.0 * sqrt_a * alpha;
                let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cw);
                let a2 = (a + 1.0) + (a - 1.0) * cw - 2.0 * sqrt_a * alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            BiquadKind::HighShelf => {
                let b0 = a * ((a + 1.0) + (a - 1.0) * cw + 2.0 * sqrt_a * alpha);
                let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cw);
                let b2 = a * ((a + 1.0) + (a - 1.0) * cw - 2.0 * sqrt_a * alpha);
                let a0 = (a + 1.0) - (a - 1.0) * cw + 2.0 * sqrt_a * alpha;
                let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cw);
                let a2 = (a + 1.0) - (a - 1.0) * cw - 2.0 * sqrt_a * alpha;
                (b0, b1, b2, a0, a1, a2)
            }
        };
        Self::new(b0, b1, b2, a0, a1, a2)
    }

    /// Process a single sample.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    /// Process a stereo pair with the same coefficients.
    #[inline]
    pub fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        (self.process(l), self.process(r))
    }

    /// Clear the filter state (does not change coefficients).
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Frequency response at `f` Hz, as (magnitude, phase in radians).
    /// Used by tests and by host-side design tools.
    pub fn response_at(&self, f: f32, fs: f32) -> (f32, f32) {
        let w = crate::util::hz_to_rad(f, fs);
        let cw = libm::cosf(w);
        let sw = libm::sinf(w);

        // H(z) = (b0 + b1 z^-1 + b2 z^-2) / (1 + a1 z^-1 + a2 z^-2)
        // with z^-1 = cos(w) - j sin(w)
        let num_re = self.b0 + self.b1 * cw + self.b2 * libm::cosf(2.0 * w);
        let num_im = -(self.b1 * sw + self.b2 * libm::sinf(2.0 * w));
        let den_re = 1.0 + self.a1 * cw + self.a2 * libm::cosf(2.0 * w);
        let den_im = -(self.a1 * sw + self.a2 * libm::sinf(2.0 * w));

        let den_sq = den_re * den_re + den_im * den_im;
        let h_re = (num_re * den_re + num_im * den_im) / den_sq;
        let h_im = (num_im * den_re - num_re * den_im) / den_sq;

        let mag = libm::sqrtf(h_re * h_re + h_im * h_im);
        let phase = libm::atan2f(h_im, h_re);
        (mag, phase)
    }

    /// Gain in dB at `f` Hz.
    pub fn gain_db_at(&self, f: f32, fs: f32) -> f32 {
        crate::util::lin_to_db(self.response_at(f, fs).0)
    }
}

/// Two biquads sharing coefficients — one per channel.
#[derive(Clone, Copy, Debug)]
pub struct StereoBiquad {
    pub left: Biquad,
    pub right: Biquad,
}

impl StereoBiquad {
    pub fn new(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self {
            left: Biquad::new(b0, b1, b2, a0, a1, a2),
            right: Biquad::new(b0, b1, b2, a0, a1, a2),
        }
    }

    pub fn make(kind: BiquadKind, fs: f32, f0: f32, q: f32, gain_db: f32) -> Self {
        Self {
            left: Biquad::make(kind, fs, f0, q, gain_db),
            right: Biquad::make(kind, fs, f0, q, gain_db),
        }
    }

    pub fn set_coefficients(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.left.set_coefficients(b0, b1, b2, a0, a1, a2);
        self.right.set_coefficients(b0, b1, b2, a0, a1, a2);
    }

    pub fn design(&mut self, kind: BiquadKind, fs: f32, f0: f32, q: f32, gain_db: f32) {
        let f = Biquad::make(kind, fs, f0, q, gain_db);
        self.set_coefficients(f.b0, f.b1, f.b2, 1.0, f.a1, f.a2);
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        (self.left.process(l), self.right.process(r))
    }

    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }
}

/// Click-free coefficient changes by crossfading two filters.
///
/// `set` starts a crossfade from the current coefficients to the new ones.
/// While the fade is running the output is `(1-t)*A + t*B`, so there is no
/// step discontinuity even though each filter's own state is preserved.
pub struct CrossfadeBiquad {
    pub a: Biquad,
    pub b: Biquad,
    t: f32,
    step: f32,
    active: bool,
}

impl CrossfadeBiquad {
    pub fn new(filter: Biquad, fade_samples: usize) -> Self {
        Self {
            a: filter,
            b: filter,
            t: 1.0,
            step: 1.0 / fade_samples.max(1) as f32,
            active: false,
        }
    }

    /// Begin a crossfade to a new design. If a fade is already running it is
    /// restarted from the current (frozen) filter, so rapid parameter changes
    /// stay click-free.
    pub fn set(&mut self, kind: BiquadKind, fs: f32, f0: f32, q: f32, gain_db: f32) {
        // Freeze the current filter (coefficients *and* state) as the fade source.
        self.a = Biquad {
            b0: self.a.b0,
            b1: self.a.b1,
            b2: self.a.b2,
            a1: self.a.a1,
            a2: self.a.a2,
            z1: self.a.z1,
            z2: self.a.z2,
        };
        self.b = Biquad::make(kind, fs, f0, q, gain_db);
        self.t = 0.0;
        self.active = true;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if !self.active {
            // Single filter path: no wasted cycles once the fade completes.
            return self.b.process(x);
        }
        let ya = self.a.process(x);
        let yb = self.b.process(x);
        let out = crate::util::lerp(ya, yb, self.t);
        self.t += self.step;
        if self.t >= 1.0 {
            self.t = 1.0;
            self.active = false;
            self.a = self.b;
        }
        out
    }

    pub fn reset(&mut self) {
        self.a.reset();
        self.b.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_response(f: &mut Biquad, n: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let x = if i == 0 { 1.0 } else { 0.0 };
            out.push(f.process(x));
        }
        out
    }

    #[test]
    fn identity_passthrough() {
        let mut f = Biquad::identity();
        assert_eq!(f.process(0.5), 0.5);
        assert_eq!(f.process(-3.0), -3.0);
    }

    #[test]
    fn lowpass_attenuates_high_frequencies() {
        let fs = 48_000.0;
        let lp = Biquad::make(BiquadKind::LowPass, fs, 1_000.0, 0.707, 0.0);
        assert!(lp.gain_db_at(100.0, fs).abs() < 0.1, "DC should pass");
        assert!(lp.gain_db_at(20_000.0, fs) < -40.0, "20 kHz should be gone");
    }

    #[test]
    fn highpass_attenuates_low_frequencies() {
        let fs = 48_000.0;
        let hp = Biquad::make(BiquadKind::HighPass, fs, 200.0, 0.707, 0.0);
        assert!(hp.gain_db_at(20.0, fs) < -35.0, "20 Hz should be gone");
        assert!(hp.gain_db_at(2_000.0, fs).abs() < 0.5, "2 kHz should pass");
    }

    #[test]
    fn notch_rejects_centre_frequency() {
        let fs = 48_000.0;
        let nt = Biquad::make(BiquadKind::Notch, fs, 1_000.0, 10.0, 0.0);
        assert!(nt.gain_db_at(1_000.0, fs) < -30.0, "notch should be deep");
        assert!(nt.gain_db_at(100.0, fs).abs() < 0.5, "far band should pass");
    }

    #[test]
    fn peak_boosts_and_cuts_symmetrically() {
        let fs = 48_000.0;
        let boost = Biquad::make(BiquadKind::Peak, fs, 1_000.0, 1.0, 6.0);
        let cut = Biquad::make(BiquadKind::Peak, fs, 1_000.0, 1.0, -6.0);
        assert!((boost.gain_db_at(1_000.0, fs) - 6.0).abs() < 0.05);
        assert!((cut.gain_db_at(1_000.0, fs) + 6.0).abs() < 0.05);
    }

    #[test]
    fn shelves_gain_at_extremes() {
        let fs = 48_000.0;
        let low = Biquad::make(BiquadKind::LowShelf, fs, 200.0, 0.707, 6.0);
        let high = Biquad::make(BiquadKind::HighShelf, fs, 5_000.0, 0.707, 6.0);
        assert!((low.gain_db_at(20.0, fs) - 6.0).abs() < 0.1);
        assert!((high.gain_db_at(20_000.0, fs) - 6.0).abs() < 0.1);
    }

    #[test]
    fn filter_is_stable_for_extreme_settings() {
        let fs = 48_000.0;
        for kind in [
            BiquadKind::LowPass,
            BiquadKind::HighPass,
            BiquadKind::BandPass,
            BiquadKind::Notch,
            BiquadKind::AllPass,
            BiquadKind::Peak,
            BiquadKind::LowShelf,
            BiquadKind::HighShelf,
        ] {
            let mut f = Biquad::make(kind, fs, 20_000.0, 10.0, 24.0);
            let h = impulse_response(&mut f, 4096);
            let tail = h[3000..].iter().map(|v| v.abs()).sum::<f32>();
            assert!(tail < 1.0, "{kind:?} did not decay: tail {tail}");
        }
    }

    #[test]
    fn crossfade_reaches_target() {
        let fs = 48_000.0;
        let mut f = CrossfadeBiquad::new(Biquad::make(BiquadKind::LowPass, fs, 200.0, 0.707, 0.0), 64);
        f.set(BiquadKind::LowPass, fs, 10_000.0, 0.707, 0.0);
        for _ in 0..64 {
            f.process(0.0);
        }
        assert!(!f.active);
        assert!((f.a.response_at(10_000.0, fs).0 - f.b.response_at(10_000.0, fs).0).abs() < 1e-6);
    }

    #[test]
    fn stereo_shares_coefficients() {
        let mut f = StereoBiquad::make(BiquadKind::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
        let (l, r) = f.process(1.0, 1.0);
        assert_eq!(l, r);
        f.design(BiquadKind::HighPass, 48_000.0, 1_000.0, 0.707, 0.0);
        assert!(f.left.gain_db_at(20.0, 48_000.0) < -40.0);
    }
}
