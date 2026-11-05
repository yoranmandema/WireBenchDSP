//! State-variable filter (Chamberlin / SVF).
//!
//! Gives low-pass, band-pass, high-pass, notch and peak outputs from a single
//! structure, with excellent stability and a musically useful resonance.
//! Preferred over a biquad when you want several outputs or smooth resonance
//! sweeps (auto-wah, filter sweeps).

use crate::util::clamp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvfOutput {
    LowPass,
    BandPass,
    HighPass,
    Notch,
    Peak,
}

pub struct Svf {
    ic1eq: f32,
    ic2eq: f32,
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl Default for Svf {
    fn default() -> Self {
        Self::new(48_000.0, 1_000.0, 0.707)
    }
}

impl Svf {
    pub fn new(sample_rate: f32, freq_hz: f32, q: f32) -> Self {
        let mut s = Self {
            ic1eq: 0.0,
            ic2eq: 0.0,
            g: 0.0,
            k: 1.0,
            a1: 1.0,
            a2: 0.0,
            a3: 0.0,
        };
        s.set_frequency(sample_rate, freq_hz, q);
        s
    }

    pub fn set_frequency(&mut self, sample_rate: f32, freq_hz: f32, q: f32) {
        let f = clamp(freq_hz, 1.0, sample_rate * 0.49);
        let q = q.max(0.01);
        self.g = libm::tanf(crate::util::PI * f / sample_rate);
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    pub fn set_q(&mut self, q: f32) {
        // Recompute with the stored g (frequency unchanged).
        let q = q.max(0.01);
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    /// Process one sample, returning all five outputs.
    #[inline]
    pub fn process_all(&mut self, x: f32) -> [f32; 5] {
        let v0 = x;
        let v3 = v0 - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.a2 * self.ic1eq + self.a3 * v3 + self.ic2eq;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        let lp = v2;
        let bp = v1;
        let hp = v0 - self.k * v1 - v2;
        let notch = hp + lp;
        let peak = lp - hp;
        [lp, bp, hp, notch, peak]
    }

    #[inline]
    pub fn process(&mut self, x: f32, output: SvfOutput) -> f32 {
        let o = self.process_all(x);
        match output {
            SvfOutput::LowPass => o[0],
            SvfOutput::BandPass => o[1],
            SvfOutput::HighPass => o[2],
            SvfOutput::Notch => o[3],
            SvfOutput::Peak => o[4],
        }
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowpass_passes_dc() {
        let mut f = Svf::new(48_000.0, 1_000.0, 0.707);
        let mut y = 0.0;
        for _ in 0..5000 {
            y = f.process(1.0, SvfOutput::LowPass);
        }
        assert!((y - 1.0).abs() < 1e-3, "LP DC gain {y}");
    }

    #[test]
    fn highpass_rejects_dc() {
        let mut f = Svf::new(48_000.0, 1_000.0, 0.707);
        let mut y = 0.0;
        for _ in 0..5000 {
            y = f.process(1.0, SvfOutput::HighPass);
        }
        assert!(y.abs() < 1e-3, "HP DC gain {y}");
    }

    #[test]
    fn resonance_boosts_cutoff() {
        let fs = 48_000.0;
        let mut low_q = Svf::new(fs, 1_000.0, 0.707);
        let mut high_q = Svf::new(fs, 1_000.0, 8.0);
        let mut peak_low = 0.0f32;
        let mut peak_high = 0.0f32;
        for i in 0..20_000 {
            let t = i as f32 / fs;
            let x = libm::sinf(2.0 * crate::util::PI * 1_000.0 * t);
            if i > 2000 {
                peak_low = peak_low.max(low_q.process(x, SvfOutput::LowPass).abs());
                peak_high = peak_high.max(high_q.process(x, SvfOutput::LowPass).abs());
            }
        }
        assert!(peak_high > peak_low, "resonance did not boost: {peak_high} vs {peak_low}");
    }

    #[test]
    fn notch_cancels_centre() {
        let fs = 48_000.0;
        let mut f = Svf::new(fs, 1_000.0, 5.0);
        let mut peak = 0.0f32;
        for i in 0..20_000 {
            let t = i as f32 / fs;
            let x = libm::sinf(2.0 * crate::util::PI * 1_000.0 * t);
            let y = f.process(x, SvfOutput::Notch);
            if i > 2000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.05, "notch leakage {peak}");
    }
}
