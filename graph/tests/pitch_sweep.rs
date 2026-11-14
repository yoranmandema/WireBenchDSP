//! US-5.3: the sweep rate must be *continuous* — cents per second, and it
//! never settles.
//!
//! Two renders through the real `PitchDelay` block:
//!
//! * a continuous sine with `feedback = 0`, so the output is a single pass
//!   through the shifter and the instantaneous pitch is directly measurable.
//!   This is where the cents-per-second mapping is checked.
//! * a short burst with `feedback > 0`, so the shifter sits in the feedback
//!   path and the repeats compound — the Rainbow-Machine sound.
//!
//! The shifter's crossfade spreads the spectrum into sidebands, so a plain
//! peak pick is unreliable on a swept signal.  The magnitude-squared spectral
//! centroid is used instead: the sidebands are symmetric about the carrier, so
//! the centroid lands on the true pitch.

use std::f32::consts::PI;

use dsp_graph::blocks::{IoInput, IoOutput, PitchDelay};
use dsp_graph::{Executor, Graph};

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;
const F0: f32 = 220.0;

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

/// Magnitude-squared spectral centroid over [lo, hi], in 1 Hz steps.
fn centroid(s: &[f32], fs: f32, lo: f32, hi: f32) -> f32 {
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    let step = 1.0f32;
    let mut f = lo;
    while f <= hi {
        let m = dft_mag(s, fs, f);
        num += f * m * m;
        den += m * m;
        f += step;
    }
    if den > 0.0 {
        num / den
    } else {
        0.0
    }
}

/// Fine-grid (1 Hz) spectral peak with parabolic refinement.
fn peak_fine(s: &[f32], fs: f32, lo: f32, hi: f32) -> f32 {
    let step = 1.0f32;
    let mut best = (0.0f32, 0.0f32);
    let mut f = lo;
    while f <= hi {
        let m = dft_mag(s, fs, f);
        if m > best.1 {
            best = (f, m);
        }
        f += step;
    }
    let (f0, m0) = best;
    let a = dft_mag(s, fs, f0 - step);
    let c = dft_mag(s, fs, f0 + step);
    let den = a - 2.0 * m0 + c;
    if den.abs() > 1e-12 {
        f0 + 0.5 * (a - c) / den * step
    } else {
        f0
    }
}

/// Render through a lone `PitchDelay` block.
///
/// `continuous` feeds a steady sine (single-pass measurement); otherwise a
/// 20 ms burst (feedback / repeats measurement).
fn render(dur_s: f32, cents: f32, rate: f32, feedback: f32, mode: f32, continuous: bool) -> Vec<f32> {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("pd", Box::new(PitchDelay::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "pd", "in").unwrap();
    g.connect("pd", "out", "out", "in").unwrap();
    g.set_param_real("pd", "time", 300.0).unwrap();
    g.set_param_real("pd", "feedback", feedback * 100.0).unwrap();
    g.set_param_real("pd", "pitch", cents).unwrap();
    g.set_param_real("pd", "rate", rate).unwrap();
    g.set_param_real("pd", "mode", mode).unwrap();
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
                if continuous || i < burst {
                    (2.0 * PI * F0 * i as f32 / SR).sin()
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

/// Cents above `F0`.
fn cents_of(f: f32) -> f32 {
    1200.0 * (f / F0).log2()
}

/// Centroid of a window of `win_s` centred at `center_s`.
fn centroid_at(out: &[f32], center_s: f32, win_s: f32) -> f32 {
    let win = (win_s * SR) as usize;
    let c = (center_s * SR) as usize;
    let lo = c.saturating_sub(win / 2);
    let hi = (c + win / 2).min(out.len());
    centroid(&out[lo..hi], SR, F0 * 0.7, F0 * 1.9)
}

/// Spectral peak of a window of `win_s` centred at `center_s`.
fn peak_at(out: &[f32], center_s: f32, win_s: f32) -> f32 {
    let win = (win_s * SR) as usize;
    let c = (center_s * SR) as usize;
    let lo = c.saturating_sub(win / 2);
    let hi = (c + win / 2).min(out.len());
    peak_fine(&out[lo..hi], SR, F0 * 0.6, F0 * 10.0)
}

/// In Sweep mode the pitch keeps climbing for as long as the gate is held: the
/// measured cents track `rate * t` and never plateau.
#[test]
fn sweep_climbs_continuously_and_never_settles() {
    let rate = 200.0f32;
    let out = render(2.0, 0.0, rate, 0.0, 1.0, true);

    let mut prev: Option<f32> = None;
    for &t in &[0.5f32, 1.0, 1.5] {
        let c = cents_of(centroid_at(&out, t, 0.15));
        let expect = rate * t;
        assert!(
            (c - expect).abs() < 60.0,
            "t={t}: measured {c:+.0} cents, expected {expect:+.0} (rate {rate} cents/s)"
        );
        if let Some(p) = prev {
            assert!(c > p, "sweep settles: {p:+.0} then {c:+.0} cents");
        }
        prev = Some(c);
    }
}

/// The `rate` parameter is cents per second: the climb over a fixed interval
/// must match `rate`, and doubling the rate must double the climb.
#[test]
fn doubling_rate_doubles_the_climb() {
    let climb = |rate: f32| -> f32 {
        let out = render(2.0, 0.0, rate, 0.0, 1.0, true);
        let f1 = centroid_at(&out, 0.5, 0.15);
        let f2 = centroid_at(&out, 1.5, 0.15);
        cents_of(f2) - cents_of(f1)
    };

    let slow = climb(200.0);
    let fast = climb(400.0);
    assert!(
        (slow - 200.0).abs() < 40.0,
        "climb {slow:.0} cents over 1.0 s, expected ~200 (cents/s)"
    );
    assert!(
        (fast - 400.0).abs() < 60.0,
        "climb {fast:.0} cents over 1.0 s, expected ~400 (cents/s)"
    );
    let ratio = fast / slow;
    assert!(
        (1.7..=2.3).contains(&ratio),
        "doubling the rate should double the climb: {slow:.0} vs {fast:.0} cents (ratio {ratio:.2})"
    );
}

/// The contrast case: `Target` mode must hold the target and ignore the rate.
/// This is what rules out a move-to-target glide.
#[test]
fn target_mode_ignores_the_rate() {
    let out = render(2.0, 600.0, 1200.0, 0.0, 0.0, true);

    let mut prev: Option<f32> = None;
    for &t in &[0.5f32, 1.0, 1.5] {
        let c = cents_of(centroid_at(&out, t, 0.15));
        assert!(
            (c - 600.0).abs() < 60.0,
            "t={t}: Target holds 600 cents, measured {c:+.0}"
        );
        if let Some(p) = prev {
            assert!(
                (c - p).abs() < 60.0,
                "Target drifts: {p:+.0} then {c:+.0} cents"
            );
        }
        prev = Some(c);
    }
}

/// With the shifter in the feedback path, the sweep compounds: each repeat is
/// higher than the last, and sits above the equivalent `Target` repeat.
#[test]
fn sweep_repeats_climb_in_the_feedback_loop() {
    let sweep = render(1.5, 600.0, 200.0, 0.6, 1.0, false);
    let target = render(1.5, 600.0, 0.0, 0.6, 0.0, false);

    let mut prev: Option<f32> = None;
    for k in 1..=4usize {
        // The first repeat lands at the nominal delay, then every 300 ms.
        let center = k as f32 * 0.300;
        let f = peak_at(&sweep, center, 0.06);
        assert!(f > 50.0, "repeat {k} is silent");
        if let Some(p) = prev {
            assert!(
                f > p * 1.1,
                "repeats do not climb: {p:.0} then {f:.0} Hz"
            );
        }
        prev = Some(f);

        let ft = peak_at(&target, center, 0.06);
        assert!(
            f > ft * 1.1,
            "repeat {k}: sweep {f:.0} Hz is not above target {ft:.0} Hz"
        );
    }
}
