//! Timing-preserving pitch shifter (phase vocoder, time-stretch + resample).
//!
//! [`delay::PitchShifter`] is a delay-line shifter: the read pointer ramps, so
//! the pitch moves but the *timing* of the signal is warped (a 5 Hz envelope
//! comes back at 4.8 Hz).  That is exactly what a pitch-shifting delay wants.
//!
//! This module is the other tool: a phase vocoder that scales every partial by
//! the same ratio while leaving the time axis alone, the way Neural DSP's
//! Parallax transpose behaves.  A 5 Hz envelope goes in, a 5 Hz envelope comes
//! out, and the pitch is shifted.
//!
//! # Why not just remap bins?
//!
//! Remapping each analysis bin to `k * ratio` and propagating the phase is a
//! *sampling* of the spectrum, not a resampling, so it aliases: whenever the
//! shifted spectrum does not land on the output bin grid, the output picks up
//! an image.  It is accurate at some frequencies and badly off at others.
//!
//! The standard fix keeps the bin grid fixed and gets the pitch shift from the
//! time axis instead:
//!
//! 1. **Time-stretch** by `ratio`: analyse at hop `H_a`, synthesise at hop
//!    `H_s = H_a * ratio`, advancing each bin's phase by `omega_k * H_s`.  The
//!    stretched signal is `ratio` times longer and its frequencies are
//!    unchanged.
//! 2. **Resample** by `ratio`: read the stretched signal `ratio` times faster.
//!    The duration is restored and every frequency is multiplied by `ratio`.
//!
//! Because the bin grid never moves, there is no spectral aliasing.
//!
//! # Latency
//!
//! The analysis window is `N` samples, so the group delay is about `N/2` —
//! 10.7 ms at 48 kHz with the default 1024.
//!
//! ```rust
//! use pedal_dsp::PitchVocoder;
//! let mut pv = PitchVocoder::<1024>::new(48_000.0);
//! pv.set_pitch_cents(1200.0); // +1 octave
//! let shifted = pv.process(0.0);
//! ```

use crate::fft::Fft;

const PI: f32 = core::f32::consts::PI;
const TAU: f32 = core::f32::consts::TAU;

/// Default analysis window, in samples.  1024 at 48 kHz is ~21 ms of window
/// and ~11 ms of group delay, which is in the same ballpark as the commercial
/// transposers this is modelled on.
pub const PITCH_VOCODER_SIZE: usize = 1024;

/// Output ring length.  Must be at least `N + H_s`; 4096 covers `N <= 1024`
/// with a pitch ratio up to 4x.
const RING: usize = 4096;

/// Timing-preserving pitch shifter.
///
/// `N` must be a power of two, at most 1024.
pub struct PitchVocoder<const N: usize> {
    fft: Fft<N>,
    window: [f32; N],
    /// Analysis hop, in samples.
    hop_a: usize,
    /// Synthesis hop, in samples: `hop_a * ratio`.
    hop_s: usize,
    in_ring: [f32; N],
    in_pos: usize,
    out_ring: [f32; RING],
    /// Integer write position for the next synthesis frame.
    write_idx: usize,
    /// Fractional read position into `out_ring`.
    read_idx: f32,
    /// FFT scratch, reused for the synthesis spectrum.
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
    /// Output magnitude per bin.
    out_mag: [f32; N],
    /// Overlap-add normalisation, indexed by `position mod hop_s`.
    ola_sum: [f32; N],
    sample_rate: f32,
    /// Current pitch shift in cents.
    pub pitch_cents: f32,
    ratio: f32,
}

impl<const N: usize> Default for PitchVocoder<N> {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl<const N: usize> PitchVocoder<N> {
    /// Create a shifter for `sample_rate`, initialised to no shift.
    ///
    /// # Panics
    /// Panics if `N` is not a power of two or is greater than 1024.
    pub fn new(sample_rate: f32) -> Self {
        assert!(
            N.is_power_of_two(),
            "PitchVocoder<N> requires N to be a power of two"
        );
        assert!(N >= 8, "PitchVocoder<N> requires N >= 8");
        assert!(N <= 1024, "PitchVocoder<N> requires N <= 1024");
        let mut window = [0.0f32; N];
        for (i, w) in window.iter_mut().enumerate() {
            // Hann.
            *w = 0.5 - 0.5 * libm::cosf(TAU * i as f32 / (N - 1) as f32);
        }
        let mut s = Self {
            fft: Fft::<N>::new(),
            window,
            hop_a: N / 4,
            hop_s: N / 4,
            in_ring: [0.0f32; N],
            in_pos: 0,
            out_ring: [0.0f32; RING],
            write_idx: 0,
            // The read pointer sits half a window ahead of the write pointer, so
            // the analysis window's centre lands on the output time axis.
            read_idx: (N / 2) as f32,
            re: [0.0f32; N],
            im: [0.0f32; N],
            prev_phase: [0.0f32; N],
            out_phase: [0.0f32; N],
            src_mag: [0.0f32; N],
            src_freq: [0.0f32; N],
            out_mag: [0.0f32; N],
            ola_sum: [1.0f32; N],
            sample_rate,
            pitch_cents: 0.0,
            ratio: 1.0,
        };
        s.recompute_ola();
        s
    }

    /// Set the pitch shift in cents (100 cents = one semitone).
    ///
    /// The change applies from the next analysis frame, so it is sample-smooth.
    pub fn set_pitch_cents(&mut self, cents: f32) {
        self.pitch_cents = cents;
        self.ratio = ratio_from_cents(cents);
        let hs = (self.hop_a as f32 * self.ratio).round() as usize;
        self.hop_s = hs.clamp(1, N);
        self.recompute_ola();
    }

    /// The analysis window length in samples.
    ///
    /// The output at time `t` is built from input samples `t-N..t`, so the
    /// group delay is roughly `N/2`.
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
        self.out_ring = [0.0f32; RING];
        self.write_idx = 0;
        self.read_idx = (N / 2) as f32;
        self.re = [0.0f32; N];
        self.im = [0.0f32; N];
        self.prev_phase = [0.0f32; N];
        self.out_phase = [0.0f32; N];
        self.src_mag = [0.0f32; N];
        self.src_freq = [0.0f32; N];
        self.out_mag = [0.0f32; N];
        self.recompute_ola();
    }

    /// Process one sample.
    pub fn process(&mut self, x: f32) -> f32 {
        self.in_ring[self.in_pos] = x;
        self.in_pos += 1;
        if self.in_pos >= N {
            self.in_pos = 0;
        }

        if self.in_pos % self.hop_a == 0 {
            self.process_frame();
        }

        self.read_output()
    }

    /// Run a block of samples through the shifter.
    pub fn process_block(&mut self, input: &mut [f32]) {
        for x in input.iter_mut() {
            *x = self.process(*x);
        }
    }

    /// Overlap-add normalisation for the current synthesis hop.
    ///
    /// The analysis and synthesis windows are both Hann, so the effective
    /// window is Hann squared.  That only sums to a constant when
    /// `hop_s == N/4`; for any other hop the sum is periodic in `hop_s`, so we
    /// precompute one period and divide by it.
    fn recompute_ola(&mut self) {
        let hs = self.hop_s;
        for tau in 0..hs {
            let mut s = 0.0f32;
            // Frames at m*hop_s contribute window^2[tau + j*hop_s] for every j
            // whose index lands inside the window.
            let mut j = 0usize;
            while tau + j * hs < N {
                let w = self.window[tau + j * hs];
                s += w * w;
                j += 1;
            }
            let mut j = 1usize;
            while tau >= j * hs {
                let w = self.window[tau - j * hs];
                s += w * w;
                j += 1;
            }
            self.ola_sum[tau] = s.max(1e-6);
        }
    }

    fn process_frame(&mut self) {
        let half = N / 2;
        let hs = self.hop_s as f32;
        // The phase deviation is measured between consecutive *analysis*
        // frames, which are `hop_a` apart — not `hop_s`. Using the synthesis
        // hop here makes the estimated bin frequency wrong by a factor that
        // depends on the wrap, so the shift lands off-pitch.
        let ha = self.hop_a as f32;

        // --- analysis -----------------------------------------------------
        for i in 0..N {
            let idx = self.in_pos + i;
            let idx = if idx >= N { idx - N } else { idx };
            self.re[i] = self.in_ring[idx] * self.window[i];
            self.im[i] = 0.0;
        }
        self.fft.transform(&mut self.re, &mut self.im);

        // --- per-bin magnitude and true frequency -------------------------
        // The expected phase advance for bin k over one synthesis hop is
        // TAU*k/N*hop_s; the deviation is the frequency offset.
        for k in 1..half {
            let mag = (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt();
            let phase = libm::atan2f(self.im[k], self.re[k]);
            let mut dphi = phase - self.prev_phase[k] - TAU * k as f32 * ha / N as f32;
            while dphi > PI {
                dphi -= TAU;
            }
            while dphi < -PI {
                dphi += TAU;
            }
            self.prev_phase[k] = phase;
            let omega = TAU * k as f32 / N as f32 + dphi / ha;
            self.src_mag[k] = mag;
            self.src_freq[k] = omega;
        }

        // --- phase propagation --------------------------------------------
        // Each bin advances at its own true frequency over the synthesis hop.
        for k in 1..half {
            self.out_phase[k] += self.src_freq[k] * hs;
            self.out_mag[k] = self.src_mag[k];
        }
        self.out_mag[0] = (self.re[0] * self.re[0] + self.im[0] * self.im[0]).sqrt();
        self.out_phase[0] = libm::atan2f(self.im[0], self.re[0]);
        self.out_mag[half] = (self.re[half] * self.re[half] + self.im[half] * self.im[half]).sqrt();
        self.out_phase[half] = libm::atan2f(self.im[half], self.re[half]);

        // --- synthesis spectrum ------------------------------------------
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

        // --- overlap-add --------------------------------------------------
        let write = self.write_idx;
        for i in 0..N {
            let idx = write + i;
            let idx = if idx >= RING { idx - RING } else { idx };
            self.out_ring[idx] += self.re[i] * self.window[i];
        }
        self.write_idx = (write + self.hop_s) % RING;
    }

    /// Read the resampled output and clear what was consumed.
    fn read_output(&mut self) -> f32 {
        let r = self.read_idx;
        let i0 = r.floor() as isize;
        let frac = r - i0 as f32;
        let i0w = i0.rem_euclid(RING as isize) as usize;
        let i1w = (i0w + 1) % RING;
        let tau = i0.rem_euclid(self.hop_s as isize) as usize;
        let norm = self.ola_sum[tau];
        let v = (self.out_ring[i0w] * (1.0 - frac) + self.out_ring[i1w] * frac) / norm;

        // Advance the read pointer by the resample ratio.  Clear only the
        // samples the *current* read consumed, i.e. up to but not including
        // the position the next read will start from (i1 = floor(r + ratio)).
        // Zeroing `i1` would clobber the first coefficient of the next
        // interpolation, producing a one-sample-per-frame dropout.
        let new_r = r + self.ratio;
        let i1 = new_r.floor() as isize;
        let mut j = i0;
        while j < i1 {
            let jw = j.rem_euclid(RING as isize) as usize;
            self.out_ring[jw] = 0.0;
            j += 1;
        }
        self.read_idx = if new_r >= RING as f32 {
            new_r - RING as f32
        } else {
            new_r
        };
        v
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

    /// Hann-windowed DFT magnitude at `f`.
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
        let mut pv = PitchVocoder::<512>::new(fs);
        let input = sine(20_000, fs, 220.0);
        let out: Vec<f32> = input.iter().map(|&x| pv.process(x)).collect();
        let a = dft_mag(&out[5000..15000], fs, 220.0);
        let b = dft_mag(&out[5000..15000], fs, 440.0);
        assert!(a > b * 20.0, "expected 220 Hz to dominate: {a} vs {b}");
    }

    #[test]
    fn shifts_up_an_octave() {
        let fs = 48_000.0f32;
        let mut pv = PitchVocoder::<1024>::new(fs);
        pv.set_pitch_cents(1200.0);
        let input = sine(40_000, fs, 220.0);
        let out: Vec<f32> = input.iter().map(|&x| pv.process(x)).collect();
        let f = peak_freq(&out[20_000..40_000], fs, 300.0, 700.0);
        let cents = 1200.0 * (f / 440.0).log2();
        assert!(cents.abs() < 10.0, "peak at {f} Hz, {cents} cents off");
    }

    #[test]
    fn shifts_down_an_octave() {
        let fs = 48_000.0f32;
        let mut pv = PitchVocoder::<1024>::new(fs);
        pv.set_pitch_cents(-1200.0);
        let input = sine(40_000, fs, 440.0);
        let out: Vec<f32> = input.iter().map(|&x| pv.process(x)).collect();
        let f = peak_freq(&out[20_000..40_000], fs, 100.0, 350.0);
        let cents = 1200.0 * (f / 220.0).log2();
        assert!(cents.abs() < 10.0, "peak at {f} Hz, {cents} cents off");
    }

    #[test]
    fn keeps_the_envelope_rate() {
        let fs = 48_000.0f32;
        let mut pv = PitchVocoder::<1024>::new(fs);
        pv.set_pitch_cents(700.0);
        let n = 40_000usize;
        let out: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / fs;
                let env = 0.5 + 0.5 * libm::sinf(TAU * 5.0 * t);
                pv.process(env * libm::sinf(TAU * 220.0 * t))
            })
            .collect();
        let seg = &out[20_000..40_000];
        let a = TAU * 20.0 / fs;
        let mut lp = 0.0f32;
        let mut env: Vec<f32> = Vec::with_capacity(seg.len());
        for &x in seg {
            lp += a * (x.abs() - lp);
            env.push(lp);
        }
        let mean = env.iter().sum::<f32>() / env.len() as f32;
        for v in env.iter_mut() {
            *v -= mean;
        }
        let f = peak_freq(&env, fs, 1.0, 20.0);
        let cents = 1200.0 * (f / 5.0).log2();
        assert!(cents.abs() < 200.0, "envelope at {f} Hz, {cents} cents off");
    }
}
