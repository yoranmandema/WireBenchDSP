//! Delay lines with fractional (interpolated) reads, feedback delay, chorus.
//!
//! Fixed-size circular buffers — no allocator, deterministic memory, safe to
//! use inside a DMA audio callback.

use crate::lfo::Lfo;
use crate::util::{clamp, lerp, mix};

pub struct DelayLine<const N: usize> {
    buf: [f32; N],
    pos: usize,
}

impl<const N: usize> Default for DelayLine<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> DelayLine<N> {
    pub fn new() -> Self {
        Self {
            buf: [0.0f32; N],
            pos: 0,
        }
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos = if self.pos + 1 >= N { 0 } else { self.pos + 1 };
    }

    /// Read `offset` samples back (0 = most recently written).
    #[inline]
    fn read_raw(&self, offset: usize) -> f32 {
        let offset = offset % N;
        // `pos` is the next write slot, so the newest sample is at pos-1.
        let idx = (self.pos + N - 1 - offset) % N;
        self.buf[idx]
    }

    /// Fractional delay with linear interpolation.
    pub fn read_at(&self, delay_samples: f32) -> f32 {
        let d = clamp(delay_samples, 0.0, (N - 1) as f32);
        let i = libm::floorf(d) as usize;
        let frac = d - i as f32;
        lerp(self.read_raw(i), self.read_raw(i + 1), frac)
    }

    pub fn clear(&mut self) {
        self.buf = [0.0f32; N];
        self.pos = 0;
    }

    pub fn capacity(&self) -> usize {
        N
    }
}

/// First-order allpass fractional delay — better than linear interpolation for
/// modulation (chorus/flanger), at the cost of one state variable.
pub struct AllpassDelayLine<const N: usize> {
    line: DelayLine<N>,
    y1: f32,
}

impl<const N: usize> Default for AllpassDelayLine<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> AllpassDelayLine<N> {
    pub fn new() -> Self {
        Self {
            line: DelayLine::new(),
            y1: 0.0,
        }
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        self.line.write(x);
    }

    /// Read with a fractional delay in samples.
    pub fn read_at(&mut self, delay_samples: f32) -> f32 {
        let d = clamp(delay_samples, 1.0, (N - 1) as f32);
        let i = libm::floorf(d) as usize;
        let frac = d - i as f32;
        // Allpass coefficient for the fractional part.
        let c = (1.0 - frac) / (1.0 + frac);
        let xn = self.line.read_raw(i);
        let xn1 = self.line.read_raw(i + 1);
        // y[n] = c*x[n] + x[n-1] - c*y[n-1]
        let y = c * xn + xn1 - c * self.y1;
        self.y1 = y;
        y
    }

    pub fn clear(&mut self) {
        self.line.clear();
        self.y1 = 0.0;
    }
}

/// Classic feedback delay (echo), with optional pitch modulation and jitter.
///
/// Pitch modulation works by wobbling the delay time: a delay whose length
/// changes over time plays back faster or slower, which is a pitch shift.
/// `pitch_cents` sets the peak swing (so the pitch moves up *and* down around
/// the base), and `jitter` adds a random wander on top.
pub struct FeedbackDelay<const N: usize> {
    pub line: DelayLine<N>,
    pub delay_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    /// Peak pitch swing in cents (-1200..+1200).  0 = no modulation.
    pub pitch_cents: f32,
    /// Random pitch wander amount, 0..1.
    pub jitter: f32,
    sample_rate: f32,
    lfo_phase: f32,
    jitter_state: f32,
    jitter_target: f32,
    rng: u32,
}

/// LFO rate (Hz) for the pitch vibrato.
const PITCH_LFO_HZ: f32 = 1.2;
/// Cap on the pitch-modulation amplitude, in samples (~42 ms at 48 kHz).
const PITCH_MOD_MAX_SAMPLES: f32 = 2000.0;

/// Small deterministic PRNG (no `rand` crate in a `no_std` audio callback).
#[inline]
fn next_random(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    ((*state >> 8) as f32 / 16_777_216.0) * 2.0 - 1.0
}

/// Convert a peak pitch swing in cents into a delay-modulation amplitude in
/// samples.  A delay `D(t) = D0 + A*sin(2*pi*f*t)` has `dD/dt = A*2*pi*f/fs`,
/// and a rate of change `r` sounds like a pitch ratio of `1 + r`, i.e.
/// `1731 * r` cents.
fn pitch_mod_samples(cents: f32, fs: f32) -> f32 {
    let a = cents.abs() * fs / (1731.0 * 2.0 * core::f32::consts::PI * PITCH_LFO_HZ);
    a.min(PITCH_MOD_MAX_SAMPLES)
}

impl<const N: usize> FeedbackDelay<N> {
    pub fn new(sample_rate: f32, delay_ms: f32) -> Self {
        Self {
            line: DelayLine::new(),
            delay_ms,
            feedback: 0.4,
            mix: 0.5,
            pitch_cents: 0.0,
            jitter: 0.0,
            sample_rate,
            lfo_phase: 0.0,
            jitter_state: 0.0,
            jitter_target: 0.0,
            rng: 0x1234_5678,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        // --- pitch vibrato: a sine LFO on the delay time ---
        let lfo = libm::sinf(core::f32::consts::TAU * self.lfo_phase);
        self.lfo_phase += PITCH_LFO_HZ / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        // A negative pitch control starts the swing downwards.
        let lfo = if self.pitch_cents < 0.0 { -lfo } else { lfo };

        // --- jitter: a smoothed random walk on the delay time ---
        self.jitter_target = clamp(self.jitter_target + next_random(&mut self.rng) * 0.06, -1.0, 1.0);
        self.jitter_state += (self.jitter_target - self.jitter_state) * 0.001;
        let jitter_samples = self.jitter_state * self.jitter * 60.0;

        let mod_samples = pitch_mod_samples(self.pitch_cents, self.sample_rate) * lfo;
        let d = self.delay_ms * 0.001 * self.sample_rate + mod_samples + jitter_samples;

        let y = self.line.read_at(d);
        self.line.write(x + y * self.feedback);
        mix(x, y, self.mix)
    }

    pub fn set_delay_ms(&mut self, ms: f32) {
        self.delay_ms = clamp(ms, 0.05, (self.line.capacity() as f32) / self.sample_rate * 1000.0);
    }

    pub fn set_feedback(&mut self, fb: f32) {
        self.feedback = clamp(fb, 0.0, 0.99);
    }

    pub fn set_pitch_cents(&mut self, cents: f32) {
        self.pitch_cents = clamp(cents, -1200.0, 1200.0);
    }

    pub fn set_jitter(&mut self, amount: f32) {
        self.jitter = clamp(amount, 0.0, 1.0);
    }

    pub fn clear(&mut self) {
        self.line.clear();
    }
}

/// Chorus / flanger: a modulated short delay.
pub struct Chorus<const N: usize> {
    pub line: AllpassDelayLine<N>,
    pub lfo: Lfo,
    pub base_ms: f32,
    pub depth_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    sample_rate: f32,
}

impl<const N: usize> Chorus<N> {
    pub fn new(sample_rate: f32, rate_hz: f32) -> Self {
        Self {
            line: AllpassDelayLine::new(),
            lfo: Lfo::new(rate_hz, sample_rate),
            base_ms: 8.0,
            depth_ms: 3.0,
            feedback: 0.0,
            mix: 0.5,
            sample_rate,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mod_ms = self.base_ms + self.depth_ms * self.lfo.process();
        let d = mod_ms * 0.001 * self.sample_rate;
        let y = self.line.read_at(d);
        self.line.write(x + y * self.feedback);
        mix(x, y, self.mix)
    }

    pub fn set_rate_hz(&mut self, hz: f32) {
        self.lfo.set_rate(hz);
    }

    pub fn clear(&mut self) {
        self.line.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_line_reads_back_samples() {
        let mut d = DelayLine::<16>::new();
        for i in 0..8 {
            d.write(i as f32);
        }
        assert_eq!(d.read_at(0.0), 7.0);
        assert_eq!(d.read_at(1.0), 6.0);
        assert_eq!(d.read_at(7.0), 0.0);
    }

    #[test]
    fn fractional_delay_interpolates() {
        let mut d = DelayLine::<16>::new();
        d.write(0.0);
        d.write(1.0);
        assert!((d.read_at(0.5) - 0.5).abs() < 1e-6);
        assert!((d.read_at(0.25) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn feedback_delay_is_stable() {
        let mut d = FeedbackDelay::<4800>::new(48_000.0, 100.0);
        d.set_feedback(0.9);
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let y = d.process(x);
            peak = peak.max(y.abs());
        }
        assert!(peak < 5.0, "feedback delay blew up: {peak}");
    }

    #[test]
    fn chorus_output_is_bounded() {
        let mut c = Chorus::<2048>::new(48_000.0, 0.5);
        c.set_rate_hz(0.5);
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let x = libm::sinf(i as f32 * 0.05);
            peak = peak.max(c.process(x).abs());
        }
        assert!(peak < 2.0, "chorus peak {peak}");
    }
}
