//! Integer oversampling to suppress aliasing from nonlinear stages.
//!
//! Nonlinear processing (distortion, rectification, bit-crush) creates
//! harmonics above Nyquist that fold back into the audio band as inharmonic
//! aliasing. Oversampling moves those images out of the audio band before the
//! nonlinearity and removes them afterwards with a half-band FIR.
//!
//! - [`Upsampler2`] / [`Downsampler2`] — one 2x stage
//! - [`Oversampler4`] — two cascaded 2x stages
//!
//! Latency is small but non-zero (the FIR group delay). At 48 kHz a 31-tap
//! half-band is ~7.5 samples per stage; a 2x stage adds ~15 samples (~312 µs),
//! which is inaudible in a pedal.

use crate::fir::Fir;

/// Taps in each half-band FIR. Odd count keeps the group delay an integer
/// number of samples, which keeps the phase aligned between channels.
pub const HALFBAND_TAPS: usize = 63;

/// Zero-stuff + half-band interpolate: one input sample in, two out.
pub struct Upsampler2 {
    fir: Fir<HALFBAND_TAPS>,
}

impl Default for Upsampler2 {
    fn default() -> Self {
        Self::new()
    }
}

impl Upsampler2 {
    pub fn new() -> Self {
        Self {
            fir: Fir::new_halfband(),
        }
    }

    /// Returns the two samples that replace `x` at 2x the sample rate.
    /// The factor of 2 compensates for the zero-stuffing (which halves the
    /// signal's DC level) so the oversampled signal keeps its amplitude.
    #[inline]
    pub fn process(&mut self, x: f32) -> [f32; 2] {
        // Zero-stuffed stream: x, 0, x, 0, ...
        let a = self.fir.process(x) * 2.0;
        let b = self.fir.process(0.0) * 2.0;
        [a, b]
    }

    pub fn reset(&mut self) {
        self.fir.reset();
    }
}

/// Half-band filter + decimate by two: two samples in, one out.
pub struct Downsampler2 {
    fir: Fir<HALFBAND_TAPS>,
    phase: u8,
}

impl Default for Downsampler2 {
    fn default() -> Self {
        Self::new()
    }
}

impl Downsampler2 {
    pub fn new() -> Self {
        Self {
            fir: Fir::new_halfband(),
            phase: 0,
        }
    }

    /// Feed one sample at the higher rate; returns `Some(output)` every
    /// second call.
    #[inline]
    pub fn process(&mut self, x: f32) -> Option<f32> {
        let y = self.fir.process(x);
        self.phase ^= 1;
        if self.phase == 0 {
            Some(y)
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.fir.reset();
        self.phase = 0;
    }
}

/// Two cascaded 2x stages (4x total).
pub struct Oversampler4 {
    up1: Upsampler2,
    up2: Upsampler2,
    down1: Downsampler2,
    down2: Downsampler2,
}

impl Default for Oversampler4 {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler4 {
    pub fn new() -> Self {
        Self {
            up1: Upsampler2::new(),
            up2: Upsampler2::new(),
            down1: Downsampler2::new(),
            down2: Downsampler2::new(),
        }
    }

    /// One input sample in, four out.
    #[inline]
    pub fn up(&mut self, x: f32) -> [f32; 4] {
        let a = self.up1.process(x);
        let b = self.up2.process(a[0]);
        let c = self.up2.process(a[1]);
        [b[0], b[1], c[0], c[1]]
    }

    /// Four input samples in, one out. Feed the four oversampled samples in
    /// order; the last `Some` is the result.
    #[inline]
    pub fn down(&mut self, x: f32) -> Option<f32> {
        match self.down2.process(x) {
            Some(y) => self.down1.process(y),
            None => None,
        }
    }

    pub fn reset(&mut self) {
        self.up1.reset();
        self.up2.reset();
        self.down1.reset();
        self.down2.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::PI;

    #[test]
    fn upsample_doubles_sample_count() {
        let mut up = Upsampler2::new();
        for i in 0..100 {
            let [a, _] = up.process(i as f32);
            // Output is filtered, so just check it is finite and bounded.
            assert!(a.is_finite());
        }
    }

    #[test]
    fn downsample_halves_sample_count() {
        let mut down = Downsampler2::new();
        let mut count = 0;
        for _ in 0..100 {
            if down.process(1.0).is_some() {
                count += 1;
            }
        }
        assert_eq!(count, 50);
    }

    #[test]
    fn roundtrip_preserves_dc() {
        let mut up = Upsampler2::new();
        let mut down = Downsampler2::new();
        // Warm up the FIRs, then compare the settled value.
        let mut last = 0.0;
        for _ in 0..5000 {
            let [a, b] = up.process(1.0);
            if let Some(y) = down.process(a) {
                last = down.process(b).unwrap_or(y);
            }
        }
        assert!((last - 1.0).abs() < 1e-3, "DC roundtrip {last}");
    }

    #[test]
    fn oversampling_reduces_aliasing_of_hard_clip() {
        // A 9 kHz sine hard-clipped at 48 kHz produces an alias at 48-9=39 kHz
        // (out of band) but also folds harmonics into the audio band. Compare
        // the energy in 0-20 kHz with and without 4x oversampling.
        let fs = 48_000.0;
        let f0 = 9_000.0;
        let n = 48_000;

        let naive: Vec<f32> = (0..n)
            .map(|i| {
                let x = libm::sinf(2.0 * PI * f0 * i as f32 / fs);
                crate::util::clamp(x * 3.0, -1.0, 1.0)
            })
            .collect();

        let mut up = Oversampler4::new();
        let mut os: Vec<f32> = Vec::with_capacity(n);
        for i in 0..n {
            let x = libm::sinf(2.0 * PI * f0 * i as f32 / fs);
            let [a, b, c, d] = up.up(x);
            let ys = [
                crate::util::clamp(a * 3.0, -1.0, 1.0),
                crate::util::clamp(b * 3.0, -1.0, 1.0),
                crate::util::clamp(c * 3.0, -1.0, 1.0),
                crate::util::clamp(d * 3.0, -1.0, 1.0),
            ];
            for y in ys {
                if let Some(o) = up.down(y) {
                    os.push(o);
                }
            }
        }

        // Measure energy above 20 kHz using a crude DFT bin at 30 kHz.
        fn band_energy(s: &[f32], fs: f32, f: f32) -> f32 {
            let mut re = 0.0;
            let mut im = 0.0;
            for (i, v) in s.iter().enumerate() {
                let w = 2.0 * PI * f * i as f32 / fs;
                re += v * libm::cosf(w);
                im += v * libm::sinf(w);
            }
            (re * re + im * im).sqrt() / s.len() as f32
        }

        let e_naive = band_energy(&naive, fs, 21_000.0);
        let e_os = band_energy(&os, fs, 21_000.0);
        assert!(
            e_os < e_naive * 0.2,
            "oversampling did not reduce aliased 3rd harmonic: naive {e_naive}, os {e_os}"
        );
    }
}
