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

/// A static pitch shifter.
///
/// The write head advances one sample per sample; the read head advances at
/// `2^(cents/1200)` samples per sample, so the delay between them changes and
/// the signal is sped up or slowed down.  Because the read head eventually
/// laps the write head, the wrap is hidden by crossfading between two heads
/// half a buffer apart (a Hann window over the buffer).
///
/// Put it in a delay's feedback path and the shift accumulates: every repeat
/// is shifted again, so the pitch climbs (or falls) -- the "shimmer" sound.
pub struct PitchShifter<const N: usize> {
    buf: [f32; N],
    wp: f32,
    /// Current delay (write minus read), in samples.  Sweeps as the pitch
    /// shifts, wrapping from N back to 0.
    sweep: f32,
    /// Pitch shift in cents.  0 = unchanged, 1200 = one octave up.
    pub pitch_cents: f32,
    /// Random pitch wander, 0..1.
    pub jitter: f32,
    jitter_state: f32,
    jitter_target: f32,
    rng: u32,
}

impl<const N: usize> Default for PitchShifter<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> PitchShifter<N> {
    pub fn new() -> Self {
        Self {
            buf: [0.0; N],
            wp: 0.0,
            sweep: (N / 2) as f32,
            pitch_cents: 0.0,
            jitter: 0.0,
            jitter_state: 0.0,
            jitter_target: 0.0,
            rng: 0x2345_6789,
        }
    }

    #[inline]
    fn read_at(&self, pos: f32) -> f32 {
        let n = N as f32;
        let p = ((pos % n) + n) % n;
        let i = p as usize;
        let frac = p - i as f32;
        let j = (i + 1) % N;
        self.buf[i] * (1.0 - frac) + self.buf[j] * frac
    }

    /// Latency this shifter adds, in samples (the sweep centres on N/2).
    #[inline]
    pub fn latency_samples(&self) -> f32 {
        (N / 2) as f32
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let n = N as f32;
        let half = (N / 2) as f32;

        // Write one sample, advance the write head.
        self.buf[self.wp as usize] = x;
        self.wp += 1.0;
        if self.wp >= n {
            self.wp -= n;
        }

        // Jitter: a smoothed random wander around the set pitch.
        self.jitter_target = clamp(
            self.jitter_target + next_random(&mut self.rng) * 0.06,
            -1.0,
            1.0,
        );
        self.jitter_state += (self.jitter_target - self.jitter_state) * 0.001;
        let cents = self.pitch_cents + self.jitter_state * self.jitter * 150.0;

        // Read rate: 1.0 = unchanged, 2.0 = octave up, 0.5 = octave down.
        // The delay changes at (1 - rate) samples per sample.
        let rate = libm::powf(2.0, cents / 1200.0);
        self.sweep += 1.0 - rate;
        self.sweep = ((self.sweep % n) + n) % n;

        // Two heads half a buffer apart.  The window is a function of the
        // DELAY, so it is 0 exactly where the read head laps the write head
        // (the wrap) and 1 halfway through the buffer.
        let w = 0.5 - 0.5 * libm::cosf(core::f32::consts::TAU * self.sweep / n);
        let a = self.read_at(self.wp - self.sweep);
        let b = self.read_at(self.wp - self.sweep - half);
        a * w + b * (1.0 - w)
    }

    pub fn clear(&mut self) {
        self.buf = [0.0; N];
        self.wp = 0.0;
        self.sweep = (N / 2) as f32;
    }
}

/// Small deterministic PRNG (no `rand` crate in a `no_std` audio callback).
#[inline]
fn next_random(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    ((*state >> 8) as f32 / 16_777_216.0) * 2.0 - 1.0
}

/// Classic feedback delay (echo) with a static pitch shifter in the loop.
///
/// `pitch_cents` shifts the delayed signal; because the shifter sits in the
/// feedback path, each repeat is shifted again and the pitch accumulates.
pub struct FeedbackDelay<const N: usize, const P: usize = N> {
    pub line: DelayLine<N>,
    pub delay_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    /// Static pitch shift in cents (-2400..+2400).  0 = plain delay.
    pub pitch_cents: f32,
    /// Random pitch wander amount, 0..1.
    pub jitter: f32,
    sample_rate: f32,
    pitch: PitchShifter<P>,
}

impl<const N: usize, const P: usize> FeedbackDelay<N, P> {
    pub fn new(sample_rate: f32, delay_ms: f32) -> Self {
        Self {
            line: DelayLine::new(),
            delay_ms,
            feedback: 0.4,
            mix: 0.5,
            pitch_cents: 0.0,
            jitter: 0.0,
            sample_rate,
            pitch: PitchShifter::new(),
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.pitch.pitch_cents = self.pitch_cents;
        self.pitch.jitter = self.jitter;
        let pitching = self.pitch_cents != 0.0 || self.jitter != 0.0;

        // Compensate for the shifter's latency so the delay time stays put.
        let mut d = self.delay_ms * 0.001 * self.sample_rate;
        if pitching {
            d -= self.pitch.latency_samples();
            if d < 1.0 {
                d = 1.0;
            }
        }

        let delayed = self.line.read_at(d);
        let y = if pitching {
            self.pitch.process(delayed)
        } else {
            delayed
        };
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
        self.pitch_cents = clamp(cents, -2400.0, 2400.0);
    }

    pub fn set_jitter(&mut self, amount: f32) {
        self.jitter = clamp(amount, 0.0, 1.0);
    }

    pub fn clear(&mut self) {
        self.line.clear();
        self.pitch.clear();
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
