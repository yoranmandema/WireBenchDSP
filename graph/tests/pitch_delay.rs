//! US-5.2: the pitch-shifting delay's repeats must climb in pitch.
//!
//! The `PitchDelay` block puts a pitch shifter in the feedback path, so every
//! repeat is shifted again.  This renders a short sine burst through the real
//! block and checks that each successive repeat sits higher than the last.

use std::f32::consts::PI;

use dsp_graph::blocks::{IoInput, IoOutput, PitchDelay};
use dsp_graph::{Executor, Graph};

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

/// Hann-windowed DFT magnitude at one frequency.
fn dft_mag(s: &[f32], fs: f32, f: f32) -> f32 {
    let n = s.len();
    let mut re = 0.0f32;
    let mut im = 0.0f32;
    for (i, &x) in s.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / (n - 1) as f32).cos();
        let ph = -2.0 * PI * f * i as f32 / fs;
        re += w * x * ph.cos();
        im += w * x * ph.sin();
    }
    (re * re + im * im).sqrt() / n as f32
}

/// Largest-magnitude frequency in [lo, hi], and its magnitude.
fn peak_freq(s: &[f32], fs: f32, lo: f32, hi: f32) -> (f32, f32) {
    let n = s.len();
    let bin = fs / n as f32;
    let k0 = (lo / bin).ceil().max(1.0) as usize;
    let k1 = (hi / bin).floor() as usize;
    let mut best = (0.0f32, 0.0f32);
    for k in k0..=k1 {
        let f = k as f32 * bin;
        let m = dft_mag(s, fs, f);
        if m > best.1 {
            best = (f, m);
        }
    }
    best
}

/// Render `dur_s` of a 20 ms sine burst through a lone `PitchDelay` block.
fn render(dur_s: f32, f0: f32, cents: f32, delay_ms: f32, feedback: f32) -> Vec<f32> {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("pd", Box::new(PitchDelay::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "pd", "in").unwrap();
    g.connect("pd", "out", "out", "in").unwrap();
    g.set_param_real("pd", "time", delay_ms).unwrap();
    g.set_param_real("pd", "feedback", feedback * 100.0)
        .unwrap();
    g.set_param_real("pd", "pitch", cents).unwrap();
    g.set_param_real("pd", "rate", 0.0).unwrap();
    g.set_param_real("pd", "mode", 0.0).unwrap();
    g.set_param_real("pd", "mix", 100.0).unwrap();
    let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();

    let total = (dur_s * SR) as usize;
    let burst = (0.02 * SR) as usize;
    let mut out = Vec::with_capacity(total);
    let mut done = 0;
    while done < total {
        let n = BLOCK.min(total - done);
        let input: Vec<f32> = (0..n)
            .map(|j| {
                let i = done + j;
                if i < burst {
                    (2.0 * PI * f0 * i as f32 / SR).sin()
                } else {
                    0.0
                }
            })
            .collect();
        out.extend_from_slice(ex.run(&input, n).unwrap());
        done += n;
    }
    out
}

/// Each repeat is the previous one shifted up by `cents` again, so the
/// measured frequencies must rise monotonically and land near the expected
/// `f0 * 2^(k*cents/1200)`.
#[test]
fn repeats_climb_in_pitch() {
    let f0 = 220.0f32;
    let cents = 600.0f32;
    let delay_ms = 300.0f32;
    let out = render(1.5, f0, cents, delay_ms, 0.7);

    let expected: Vec<f32> = (1..=4)
        .map(|k| f0 * (2.0f32).powf(k as f32 * cents / 1200.0))
        .collect();

    let mut measured = Vec::new();
    for k in 1..=4usize {
        let center = (k as f32 * delay_ms / 1000.0 * SR) as usize;
        let half = (0.06 * SR) as usize;
        let lo = center.saturating_sub(half);
        let hi = (center + half).min(out.len());
        let (f, m) = peak_freq(&out[lo..hi], SR, 100.0, 2000.0);
        assert!(m > 1e-4, "repeat {k} is silent");
        measured.push(f);
        let ratio = f / expected[k - 1];
        assert!(
            (0.85..=1.18).contains(&ratio),
            "repeat {k}: measured {f:.1} Hz, expected ~{:.1} Hz (ratio {ratio:.2})",
            expected[k - 1]
        );
    }

    for w in measured.windows(2) {
        assert!(w[1] > w[0] * 1.1, "repeats do not climb: {measured:?}");
    }
}
