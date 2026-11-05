//! Windowed-sinc FIR filter, used for oversampling (anti-imaging / anti-aliasing).
//!
//! Fixed-size const-generic implementation: no allocator, no dynamic state.
//! The design is a Blackman-windowed sinc; coefficients are normalised for
//! unity DC gain so the oversampled signal keeps its level.

use crate::util::{clamp01, PI};

pub struct Fir<const N: usize> {
    coeffs: [f32; N],
    state: [f32; N],
    pos: usize,
}

impl<const N: usize> Fir<N> {
    /// Low-pass with cutoff `cutoff_norm` (0..1, relative to Nyquist).
    pub fn new_lowpass(cutoff_norm: f32) -> Self {
        assert!(N >= 3, "FIR needs at least 3 taps");
        let cutoff = clamp01(cutoff_norm);
        let m = (N - 1) as f32 * 0.5;
        let mut coeffs = [0.0f32; N];
        let mut sum = 0.0f32;

        for n in 0..N {
            let x = n as f32 - m;
            // `cutoff_norm` is relative to Nyquist, so fc (relative to the
            // sample rate) is cutoff_norm / 2.  h[n] = sin(pi*fc_n*x)/(pi*x).
            let sinc = if x.abs() < 1e-9 {
                cutoff
            } else {
                libm::sinf(PI * cutoff * x) / (PI * x)
            };
            // Blackman window
            let denom = (N - 1) as f32;
            let w = 0.42 - 0.5 * libm::cosf(2.0 * PI * n as f32 / denom)
                + 0.08 * libm::cosf(4.0 * PI * n as f32 / denom);
            let h = sinc * w;
            coeffs[n] = h;
            sum += h;
        }
        // Normalise unity DC gain.
        if sum.abs() > 1e-12 {
            for c in coeffs.iter_mut() {
                *c /= sum;
            }
        }
        Self {
            coeffs,
            state: [0.0f32; N],
            pos: 0,
        }
    }

    /// Half-band low-pass (cutoff at 0.5 of Nyquist) — the standard
    /// anti-imaging / anti-aliasing filter for integer oversampling.
    pub fn new_halfband() -> Self {
        Self::new_lowpass(0.5)
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.state[self.pos] = x;
        self.pos = if self.pos + 1 >= N { 0 } else { self.pos + 1 };

        let mut acc = 0.0f32;
        let mut idx = self.pos; // oldest sample
        for c in self.coeffs.iter() {
            acc += c * self.state[idx];
            idx = if idx + 1 >= N { 0 } else { idx + 1 };
        }
        acc
    }

    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        let n = input.len().min(output.len());
        for i in 0..n {
            output[i] = self.process(input[i]);
        }
    }

    pub fn reset(&mut self) {
        self.state = [0.0f32; N];
        self.pos = 0;
    }

    pub fn group_delay_samples(&self) -> f32 {
        (N - 1) as f32 * 0.5
    }

    pub fn coeffs(&self) -> &[f32; N] {
        &self.coeffs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_gain_is_unity() {
        let mut f = Fir::<31>::new_halfband();
        let mut y = 0.0;
        for _ in 0..2000 {
            y = f.process(1.0);
        }
        assert!((y - 1.0).abs() < 1e-4, "DC gain {y}");
    }

    #[test]
    fn impulse_is_symmetric() {
        let mut f = Fir::<31>::new_halfband();
        let mut h = [0.0f32; 64];
        for i in 0..64 {
            h[i] = f.process(if i == 0 { 1.0 } else { 0.0 });
        }
        // Linear phase => symmetric about the centre tap.
        let c = 15;
        for k in 1..=15 {
            assert!((h[c + k] - h[c - k]).abs() < 1e-6, "asymmetric at {k}");
        }
    }

    #[test]
    fn halfband_kills_the_image_band() {
        let mut f = Fir::<63>::new_halfband();
        // A signal at 0.75 of Nyquist is well into the image band and must be
        // strongly rejected. (Exactly at Nyquist the gain is 0.5 by design —
        // the transition band is centred on the cutoff.)
        let fs = 48_000.0;
        let mut peak = 0.0f32;
        for i in 0..8000 {
            let t = i as f32 / fs;
            let x = libm::sinf(2.0 * PI * 0.75 * (fs * 0.5) * t);
            let y = f.process(x);
            if i > 500 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.02, "image band leakage {peak}");
    }

    #[test]
    fn lowpass_attenuates_above_cutoff() {
        let mut f = Fir::<63>::new_lowpass(0.25);
        // 0.25 of Nyquist = 6 kHz at 48 kHz. A 12 kHz sine should be strongly reduced.
        let fs = 48_000.0;
        let mut peak = 0.0f32;
        for i in 0..8000 {
            let t = i as f32 / fs;
            let x = libm::sinf(2.0 * PI * 12_000.0 * t);
            let y = f.process(x);
            if i > 500 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.1, "12 kHz leakage {peak}");
    }
}
