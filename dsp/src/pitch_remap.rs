//! Timing-preserving pitch shifter by spectral bin remapping.
//!
//! This is the simpler of the two vocoders in this crate.  It takes the
//! windowed STFT, computes each bin's true frequency from the phase difference
//! between frames, moves the magnitude to `k * ratio`, and advances the phase
//! at the shifted frequency.  Overlap-add then turns it back into audio.
//!
//! # Known limitation
//!
//! Remapping bins is a *sampling* of the spectrum, not a resampling: the
//! shifted spectrum is read off the output bin grid, so it aliases whenever
//! the result does not land on that grid.  In practice this is accurate to
//! within ~1 cent at many frequencies (80 Hz – 1760 Hz, up and down an octave)
//! but can be badly off at others — 220 Hz → 440 Hz produces a strong 252 Hz
//! image, for example.
//!
//! [`crate::pitch_vocoder::PitchVocoder`] uses the time-stretch + resample
//! approach and does not have this problem; it is the one to use.  This module
//! is kept because the remap version is simpler, cheaper, and fine for
//! applications where the aliasing is not objectionable.
//!
//! ```rust
//! use pedal_dsp::PitchRemap;
//! let mut pr = PitchRemap::<1024>::new(48_000.0);
//! pr.set_pitch_cents(700.0);
//! let shifted = pr.process(0.0);
//! ```

use crate::fft::Fft;

const PI: f32 = core::f32::consts::PI;
const TAU: f32 = core::f32::consts::TAU;

/// Timing-preserving pitch shifter by bin remap.
///
/// `N` must be a power of two.  The hop is `N/4`.
pub struct PitchRemap<const N: usize> {
    fft: Fft<N>,
    window: [f32; N],
    hop: usize,
    in_ring: [f32; N],
    in_pos: usize,
    out_ring: [f32; N],
    /// Output read position, offset by `N/2` from the write position so the
    /// analysis window's centre lands on the output time axis.
    out_read: usize,
    re: [f32; N],
    im: [f32; N],
    /// Previous analysis phase per bin (radians).
    prev_phase: [f32; N],
    /// Accumulated synthesis phase per bin (radians).
    out_phase: [f32; N],
    /// Per-bin source magnitude, scratch.
    src_mag: [f32; N],
    /// Per-bin source true frequency (rad/sample), scratch.
    src_freq: [f32; N],
    /// Accumulated difference between the source phase advance and the output
    /// phase advance, so the output stays locked to the source phase.
    phase_offset: [f32; N],
    /// Output magnitude per bin.
    out_mag: [f32; N],
    sample_rate: f32,
    /// Current pitch shift in cents.
    pub pitch_cents: f32,
    ratio: f32,
}

impl<const N: usize> Default for PitchRemap<N> {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl<const N: usize> PitchRemap<N> {
    /// Create a shifter for `sample_rate`, initialised to no shift.
    ///
    /// # Panics
    /// Panics if `N` is not a power of two or is less than 8.
    pub fn new(sample_rate: f32) -> Self {
        assert!(N.is_power_of_two(), "PitchRemap<N> requires N to be a power of two");
        assert!(N >= 8, "PitchRemap<N> requires N >= 8");
        let mut window = [0.0f32; N];
        for (i, w) in window.iter_mut().enumerate() {
            *w = 0.5 - 0.5 * libm::cosf(TAU * i as f32 / (N - 1) as f32);
        }
        let mut s = Self {
            fft: Fft::<N>::new(),
            window,
            hop: N / 4,
            in_ring: [0.0f32; N],
            in_pos: 0,
            out_ring: [0.0f32; N],
            out_read: N / 2,
            re: [0.0f32; N],
            im: [0.0f32; N],
            prev_phase: [0.0f32; N],
            out_phase: [0.0f32; N],
            src_mag: [0.0f32; N],
            src_freq: [0.0f32; N],
            phase_offset: [0.0f32; N],
            out_mag: [0.0f32; N],
            sample_rate,
            pitch_cents: 0.0,
            ratio: 1.0,
        };
        s.ratio = ratio_from_cents(0.0);
        s
    }

    /// Set the pitch shift in cents (100 cents = one semitone).
    ///
    /// The change applies from the next analysis frame, so it is sample-smooth.
    pub fn set_pitch_cents(&mut self, cents: f32) {
        self.pitch_cents = cents;
        self.ratio = ratio_from_cents(cents);
    }

    /// The analysis window length in samples.
    pub fn latency_samples(&self) -> usize {
        N
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    /// Clear all state.
    pub fn reset(&mut self) {
        self.in_ring = [0.0f32; N];
        self.in_pos = 0;
        self.out_ring = [0.0f32; N];
        self.out_read = N / 2;
        self.re = [0.0f32; N];
        self.im = [0.0f32; N];
        self.prev_phase = [0.0f32; N];
        self.out_phase = [0.0f32; N];
        self.src_mag = [0.0f32; N];
        self.src_freq = [0.0f32; N];
        self.phase_offset = [0.0f32; N];
        self.out_mag = [0.0f32; N];
    }

    /// Process one sample.
    pub fn process(&mut self, x: f32) -> f32 {
        self.in_ring[self.in_pos] = x;
        self.in_pos += 1;
        if self.in_pos >= N {
            self.in_pos = 0;
        }

        if self.in_pos % self.hop == 0 {
            self.process_frame();
        }

        let y = self.out_ring[self.out_read];
        self.out_ring[self.out_read] = 0.0;
        self.out_read += 1;
        if self.out_read >= N {
            self.out_read = 0;
        }
        y
    }

    /// Run a block of samples through the shifter.
    pub fn process_block(&mut self, input: &mut [f32]) {
        for x in input.iter_mut() {
            *x = self.process(*x);
        }
    }

    fn process_frame(&mut self) {
        let half = N / 2;

        // --- analysis ---
        for i in 0..N {
            let idx = self.in_pos + i;
            let idx = if idx >= N { idx - N } else { idx };
            self.re[i] = self.in_ring[idx] * self.window[i];
            self.im[i] = 0.0;
        }
        self.fft.transform(&mut self.re, &mut self.im);

        // --- per-bin magnitude and true frequency ---
        for k in 1..half {
            let mag = (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt();
            let phase = libm::atan2f(self.im[k], self.re[k]);
            let mut dphi =
                phase - self.prev_phase[k] - TAU * k as f32 * self.hop as f32 / N as f32;
            // Wrap the *deviation* into [-pi, pi].
            while dphi > PI {
                dphi -= TAU;
            }
            while dphi < -PI {
                dphi += TAU;
            }
            self.prev_phase[k] = phase;
            let omega = TAU * k as f32 / N as f32 + dphi / self.hop as f32;
            self.src_mag[k] = mag;
            self.src_freq[k] = omega;
        }

        // --- remap to the shifted bins ---
        for k_out in 1..half {
            let ks = k_out as f32 / self.ratio;
            let mut k0 = ks.floor() as usize;
            if k0 >= half {
                k0 = half - 1;
            }
            let frac = ks - k0 as f32;
            let k1 = if k0 + 1 < half { k0 + 1 } else { half - 1 };

            let mag = self.src_mag[k0] + (self.src_mag[k1] - self.src_mag[k0]) * frac;
            let omega = self.src_freq[k0] + (self.src_freq[k1] - self.src_freq[k0]) * frac;
            let omega_out = omega * self.ratio;
            // Anchor the output phase to the source phase, then accumulate the
            // difference between the source's advance and the output's advance.
            self.out_phase[k_out] = self.prev_phase[k0] + self.phase_offset[k_out];
            self.phase_offset[k_out] += (omega_out - self.src_freq[k0]) * self.hop as f32;
            self.out_mag[k_out] = mag;
        }

        // DC and Nyquist pass through unchanged (a ratio does not move them).
        self.out_mag[0] = (self.re[0] * self.re[0] + self.im[0] * self.im[0]).sqrt();
        self.out_phase[0] = libm::atan2f(self.im[0], self.re[0]);
        self.out_mag[half] = (self.re[half] * self.re[half] + self.im[half] * self.im[half]).sqrt();
        self.out_phase[half] = libm::atan2f(self.im[half], self.re[half]);

        // --- synthesis spectrum ---
        for k in 0..=half {
            self.re[k] = self.out_mag[k] * libm::cosf(self.out_phase[k]);
            self.im[k] = self.out_mag[k] * libm::sinf(self.out_phase[k]);
        }
        for k in 1..half {
            let m = self.out_mag[k];
            let p = -self.out_phase[k];
            self.re[N - k] = m * libm::cosf(p);
            self.im[N - k] = m * libm::sinf(p);
        }
        self.fft.inverse(&mut self.re, &mut self.im);

        // --- overlap-add ---
        let write = self.in_pos;
        for i in 0..N {
            let idx = write + i;
            let idx = if idx >= N { idx - N } else { idx };
            self.out_ring[idx] += self.re[i] * self.window[i];
        }
    }
}

fn ratio_from_cents(cents: f32) -> f32 {
    libm::powf(2.0, cents / 1200.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(n: usize, fs: f32, f: f32) -> Vec<f32> {
        (0..n).map(|i| libm::sinf(TAU * f * i as f32 / fs)).collect()
    }

    fn dft_mag(s: &[f32], fs: f32, f: f32) -> f32 {
        let n = s.len();
        let mut re = 0.0f32;
        let mut im = 0.0f32;
        for (i, &x) in s.iter().enumerate() {
            let w = 0.5 - 0.5 * libm::cosf(TAU * i as f32 / (n - 1) as f32);
            let ph = -TAU * f * i as f32 / fs;
            re += w * x * libm::cosf(ph);
            im += w * x * libm::sinf(ph);
        }
        (re * re + im * im).sqrt()
    }

    fn peak_freq(s: &[f32], fs: f32, lo: f32, hi: f32) -> f32 {
        let n = s.len();
        let k_lo = (lo / fs * n as f32).ceil() as usize;
        let k_hi = (hi / fs * n as f32).floor() as usize;
        let mut best_k = k_lo;
        let mut best_m = -1.0f32;
        for k in k_lo..=k_hi.min(n / 2 - 1) {
            let f = k as f32 * fs / n as f32;
            let m = dft_mag(s, fs, f);
            if m > best_m {
                best_m = m;
                best_k = k;
            }
        }
        if best_k > k_lo && best_k < k_hi {
            let a = dft_mag(s, fs, (best_k - 1) as f32 * fs / n as f32).ln();
            let b = best_m.ln();
            let c = dft_mag(s, fs, (best_k + 1) as f32 * fs / n as f32).ln();
            let denom = a - 2.0 * b + c;
            if denom.abs() > 1e-9 {
                let delta = 0.5 * (a - c) / denom;
                return (best_k as f32 + delta) * fs / n as f32;
            }
        }
        best_k as f32 * fs / n as f32
    }

    #[test]
    fn unity_passes_the_signal_through() {
        let fs = 48_000.0f32;
        let mut pr = PitchRemap::<512>::new(fs);
        let input = sine(20_000, fs, 220.0);
        let out: Vec<f32> = input.iter().map(|&x| pr.process(x)).collect();
        let a = dft_mag(&out[5000..15000], fs, 220.0);
        let b = dft_mag(&out[5000..15000], fs, 440.0);
        assert!(a > b * 20.0, "expected 220 Hz to dominate: {a} vs {b}");
    }

    #[test]
    fn shifts_down_an_octave() {
        let fs = 48_000.0f32;
        let mut pr = PitchRemap::<1024>::new(fs);
        pr.set_pitch_cents(-1200.0);
        let input = sine(40_000, fs, 440.0);
        let out: Vec<f32> = input.iter().map(|&x| pr.process(x)).collect();
        let f = peak_freq(&out[20_000..40_000], fs, 100.0, 350.0);
        let cents = 1200.0 * (f / 220.0).log2();
        assert!(cents.abs() < 10.0, "peak at {f} Hz, {cents} cents off");
    }
}
