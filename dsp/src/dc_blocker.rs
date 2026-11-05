//! DC offset removal.
//!
//! A guitar signal through a nonlinear pedal can pick up a DC component; a
//! DC blocker at the input and/or output keeps the signal centred and stops
//! the "thump" when switching effects.

use crate::util::clamp;

pub struct DcBlocker {
    x1: f32,
    y1: f32,
    coef: f32,
}

impl Default for DcBlocker {
    fn default() -> Self {
        Self::new(20.0, 48_000.0)
    }
}

impl DcBlocker {
    /// `cutoff_hz` is the -3 dB point of the high-pass (20 Hz is a good
    /// choice for guitar: keeps the low E at 82 Hz intact).
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let coef = 1.0 - crate::util::TAU * clamp(cutoff_hz, 1.0, sample_rate * 0.49) / sample_rate;
        Self {
            x1: 0.0,
            y1: 0.0,
            coef,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.coef * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_is_removed() {
        let mut b = DcBlocker::new(20.0, 48_000.0);
        let mut y = 0.0;
        for _ in 0..10_000 {
            y = b.process(1.0);
        }
        assert!(y.abs() < 1e-3, "DC not removed: {y}");
    }

    #[test]
    fn audio_passes() {
        let mut b = DcBlocker::new(20.0, 48_000.0);
        let fs = 48_000.0;
        let mut peak = 0.0f32;
        for i in 0..20_000 {
            let x = libm::sinf(2.0 * crate::util::PI * 440.0 * i as f32 / fs);
            let y = b.process(x);
            if i > 1000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak > 0.9, "440 Hz attenuated too much: {peak}");
    }

    #[test]
    fn low_e_survives() {
        let mut b = DcBlocker::new(20.0, 48_000.0);
        let fs = 48_000.0;
        let mut peak = 0.0f32;
        for i in 0..20_000 {
            let x = libm::sinf(2.0 * crate::util::PI * 82.4 * i as f32 / fs);
            let y = b.process(x);
            if i > 1000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak > 0.95, "82 Hz too attenuated: {peak}");
    }
}
