//! Low-frequency oscillators for modulation (tremolo, chorus, vibrato,
//! auto-wah, filter sweeps).

use crate::util::{clamp01, wrap01, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LfoShape {
    Sine,
    Triangle,
    Saw,
    Square,
    SampleHold,
}

pub struct Lfo {
    pub rate_hz: f32,
    pub shape: LfoShape,
    sample_rate: f32,
    phase: f32,
    held: f32,
}

impl Lfo {
    pub fn new(rate_hz: f32, sample_rate: f32) -> Self {
        Self {
            rate_hz: rate_hz.max(0.01),
            shape: LfoShape::Sine,
            sample_rate,
            phase: 0.0,
            held: 0.0,
        }
    }

    pub fn set_rate(&mut self, hz: f32) {
        self.rate_hz = hz.max(0.01);
    }

    pub fn set_shape(&mut self, shape: LfoShape) {
        self.shape = shape;
    }

    /// Advance one sample and return the modulator in roughly -1..1.
    #[inline]
    pub fn process(&mut self) -> f32 {
        let inc = self.rate_hz / self.sample_rate;
        let out = match self.shape {
            LfoShape::Sine => libm::sinf(TAU * self.phase),
            LfoShape::Triangle => {
                2.0 * (2.0 * (self.phase - libm::floorf(self.phase)) - 1.0).abs() - 1.0
            }
            LfoShape::Saw => 2.0 * self.phase - 1.0,
            LfoShape::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleHold => {
                let new_phase = wrap01(self.phase + inc);
                if new_phase < self.phase {
                    // Deterministic pseudo-random step.
                    let s = libm::sinf(self.phase * 12345.678) * 43758.5453;
                    self.held = (s - libm::floorf(s)) * 2.0 - 1.0;
                }
                self.held
            }
        };
        self.phase = wrap01(self.phase + inc);
        out
    }

    /// 0..1 version (useful for mapping to a parameter range).
    #[inline]
    pub fn process01(&mut self) -> f32 {
        clamp01(self.process() * 0.5 + 0.5)
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.held = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_stays_in_range() {
        let mut l = Lfo::new(5.0, 48_000.0);
        for _ in 0..10_000 {
            let v = l.process();
            assert!(v >= -1.0001 && v <= 1.0001, "sine out of range: {v}");
        }
    }

    #[test]
    fn saw_sweeps_full_range() {
        let mut l = Lfo::new(5.0, 48_000.0);
        l.set_shape(LfoShape::Saw);
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for _ in 0..10_000 {
            let v = l.process();
            min = min.min(v);
            max = max.max(v);
        }
        assert!(min < -0.99 && max > 0.99, "saw range {min}..{max}");
    }

    #[test]
    fn square_is_bipolar() {
        let mut l = Lfo::new(5.0, 48_000.0);
        l.set_shape(LfoShape::Square);
        for _ in 0..1000 {
            let v = l.process();
            assert!(v == 1.0 || v == -1.0, "square value {v}");
        }
    }

    #[test]
    fn rate_controls_period() {
        // 10 Hz at 48 kHz should complete ~10 cycles per second.
        let mut l = Lfo::new(10.0, 48_000.0);
        let mut crossings = 0;
        let mut prev = l.process();
        for _ in 0..200_000 {
            let v = l.process();
            if prev < 0.0 && v >= 0.0 {
                crossings += 1;
            }
            prev = v;
        }
        assert!(crossings >= 35 && crossings <= 50, "crossings {crossings}");
    }
}
