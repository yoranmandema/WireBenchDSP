//! Measure the pitch shifter's latency, pitch accuracy and artifacts, so it can
//! be compared against a black-boxed commercial reference (Neural DSP Parallax
//! X transpose: ~12-20 ms latency, exact pitch, timing preserved).
//!
//! Run: cargo run --release --example pitch_probe

use pedal_dsp::{FeedbackDelay, PitchMode, PitchShifter, DEFAULT_SAMPLE_RATE};

fn dft_mag(s: &[f32], fs: f32, f: f32) -> f32 {
    let n = s.len() as f32;
    let mut re = 0.0f32;
    let mut im = 0.0f32;
    for (i, &x) in s.iter().enumerate() {
        let ph = -2.0 * std::f32::consts::PI * f * i as f32 / fs;
        re += x * ph.cos();
        im += x * ph.sin();
    }
    (re * re + im * im).sqrt() / n
}

/// Dominant frequency by parabolic interpolation around the DFT peak.
fn dominant_freq(s: &[f32], fs: f32, fmin: f32, fmax: f32) -> (f32, f32) {
    let n = s.len();
    let mut best = (0.0f32, 0.0f32);
    let mut k0 = (fmin * n as f32 / fs) as usize;
    let mut k1 = (fmax * n as f32 / fs) as usize;
    k0 = k0.max(1);
    k1 = k1.min(n / 2);
    let mut mag = vec![0.0f32; n / 2];
    let win: Vec<f32> = (0..n)
        .map(|i| {
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos()
        })
        .collect();
    for k in k0..k1 {
        let mut re = 0.0f32;
        let mut im = 0.0f32;
        for (i, &x) in s.iter().enumerate() {
            let ph = -2.0 * std::f32::consts::PI * k as f32 * i as f32 / n as f32;
            let w = win[i] * x;
            re += w * ph.cos();
            im += w * ph.sin();
        }
        mag[k] = (re * re + im * im).sqrt();
        if mag[k] > best.1 {
            best = (k as f32 * fs / n as f32, mag[k]);
        }
    }
    // parabolic refine
    let k = (best.0 * n as f32 / fs) as usize;
    if k > 0 && k + 1 < mag.len() {
        let a = mag[k - 1];
        let b = mag[k];
        let c = mag[k + 1];
        let d = 0.5 * (a - c) / (a - 2.0 * b + c + 1e-12);
        return (best.0 + d * fs / n as f32, best.1);
    }
    best
}

/// Latency, pitch accuracy and out-of-band energy for one buffer size.
fn measure_buffer<const N: usize>(fs: f32, f0: f32, cents: f32) {
    let mut s = PitchShifter::<N>::new(fs);
    s.pitch_cents = cents;
    s.set_pitch_glide_ms(0.0);

    // impulse latency
    let mut peak_idx = 0usize;
    let mut peak = 0.0f32;
    for i in 0..N {
        let x = if i == 0 { 1.0f32 } else { 0.0 };
        let y = s.process(x);
        if y.abs() > peak {
            peak = y.abs();
            peak_idx = i;
        }
    }

    // sine accuracy + artifacts
    let expect = f0 * (2.0f32).powf(cents / 1200.0);
    let n = 16384usize;
    let mut buf = vec![0.0f32; n];
    let mut idx = 0usize;
    for i in 0..(N + n) {
        let x = (2.0 * std::f32::consts::PI * f0 * i as f32 / fs).sin();
        let y = s.process(x);
        if i >= N {
            buf[idx] = y;
            idx += 1;
        }
    }
    let (pf, pm) = dominant_freq(&buf, fs, expect * 0.7, expect * 1.4);
    let err = 1200.0 * (pf / expect).log2();
    // Artifact indicator: energy at the 2nd harmonic and at 3x, relative to peak.
    let h2 = dft_mag(&buf, fs, expect * 2.0) / (pm + 1e-12);
    let h3 = dft_mag(&buf, fs, expect * 3.0) / (pm + 1e-12);
    println!(
        "  N={N:>5} ({:>5.1} ms buffer): latency {:>5.1} ms, peak {pf:>7.2} Hz (err {err:>+6.1} cents), H2 {h2:.3} H3 {h3:.3}",
        N as f32 / fs * 1000.0,
        peak_idx as f32 / fs * 1000.0,
    );
}

fn main() {
    let fs = DEFAULT_SAMPLE_RATE;
    println!("=== PitchShifter direct probe (fs = {fs} Hz) ===\n");

    // ---- latency: impulse response ----
    println!("-- impulse response latency --");
    for cents in [0.0f32, -1200.0, 1200.0] {
        let mut s = PitchShifter::<2400>::new(fs);
        s.pitch_cents = cents;
        s.set_pitch_glide_ms(0.0);
        let mut onset = None;
        let mut peak = 0.0f32;
        let mut peak_idx = 0usize;
        let mut energy = 0.0f32;
        let mut centroid = 0.0f32;
        let n = 48_000usize;
        for i in 0..n {
            let x = if i == 0 { 1.0f32 } else { 0.0 };
            let y = s.process(x);
            if onset.is_none() && y.abs() > 1e-4 {
                onset = Some(i);
            }
            if y.abs() > peak {
                peak = y.abs();
                peak_idx = i;
            }
            energy += y * y;
            centroid += i as f32 * y * y;
        }
        let cen = if energy > 0.0 { centroid / energy } else { 0.0 };
        println!(
            "  {cents:>7.0} cents: onset {:>6} samples ({:>5.1} ms), peak@{:>6} ({:>5.1} ms), centroid {:>6.1} samples ({:>5.1} ms), peak amp {:.4}",
            onset.unwrap_or(0),
            onset.unwrap_or(0) as f32 / fs * 1000.0,
            peak_idx,
            peak_idx as f32 / fs * 1000.0,
            cen,
            cen / fs * 1000.0,
            peak
        );
    }

    // ---- pitch accuracy ----
    println!("\n-- pitch accuracy (sine in, DFT out) --");
    let f0 = 220.0f32;
    for cents in [0.0f32, 100.0, 300.0, 600.0, 1200.0, -300.0, -600.0, -1200.0, 2400.0, -2400.0] {
        let mut s = PitchShifter::<2400>::new(fs);
        s.pitch_cents = cents;
        s.set_pitch_glide_ms(0.0);
        let mut out = vec![0.0f32; 96_000];
        for i in 0..96_000 {
            let x = (2.0 * std::f32::consts::PI * f0 * i as f32 / fs).sin();
            out[i] = s.process(x);
        }
        let expect = f0 * (2.0f32).powf(cents / 1200.0);
        let lo = expect * 0.5;
        let hi = expect * 1.5;
        let (f, m) = dominant_freq(&out[20_000..90_000], fs, lo, hi);
        let err_cents = 1200.0 * (f / expect).log2();
        let rms = out[20_000..90_000].iter().map(|v| v * v).sum::<f32>() / 70_000.0;
        println!(
            "  {cents:>7.0} cents: out {f:>8.2} Hz (expect {expect:>8.2}), err {err_cents:>+7.1} cents, mag {m:.4}, rms {rms:.4}",
        );
    }

    // ---- timing preservation: AM envelope ----
    println!("\n-- AM envelope (5 Hz on 440 Hz) --");
    {
        let mut s = PitchShifter::<2400>::new(fs);
        s.pitch_cents = 1200.0;
        s.set_pitch_glide_ms(0.0);
        let fm = 5.0f32;
        let mut out = vec![0.0f32; 96_000];
        for i in 0..96_000 {
            let env = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * fm * i as f32 / fs).sin();
            let x = env * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / fs).sin();
            out[i] = s.process(x);
        }
        // envelope of the output via rectification + LP
        let mut e = vec![0.0f32; 96_000];
        let mut lp = 0.0f32;
        for i in 0..96_000 {
            lp += (out[i].abs() - lp) * 0.002;
            e[i] = lp;
        }
        // dominant envelope frequency (1..50 Hz only)
        let n = 60_000usize;
        let seg = &e[20_000..20_000 + n];
        let bin_hz = fs / n as f32;
        let kmin = (1.0 / bin_hz).ceil() as usize;
        let kmax = (50.0 / bin_hz).floor() as usize;
        let mut best = (0.0f32, 0.0f32);
        for k in kmin..=kmax {
            let f = k as f32 * bin_hz;
            let m = dft_mag(seg, fs, f);
            if m > best.1 {
                best = (f, m);
            }
        }
        println!(
            "  envelope fundamental: {:.2} Hz (input 5.00 Hz) -> {}",
            best.0,
            if (best.0 - 5.0).abs() < 0.5 { "PRESERVED" } else { "SCALED" }
        );
    }

    // ---- FeedbackDelay: echo timing with and without pitch ----
    println!("\n-- FeedbackDelay echo timing (impulse in, delay 300 ms) --");
    for cents in [-2400.0f32, -1200.0, -600.0, 0.0, 600.0, 1200.0, 2400.0] {
        let mut d = FeedbackDelay::<48000, 2400>::new(fs, 300.0);
        d.set_pitch_cents(cents);
        d.set_pitch_mode(PitchMode::Target);
        d.set_pitch_glide_ms(0.0);
        d.delay_ms = 300.0;
        d.feedback = 0.0;
        d.mix = 1.0;
        let mut first = None;
        let mut peak = 0.0f32;
        let mut peak_idx = 0usize;
        for i in 0..48_000 {
            let x = if i == 0 { 1.0f32 } else { 0.0 };
            let y = d.process(x);
            if first.is_none() && y.abs() > 1e-4 {
                first = Some(i);
            }
            if y.abs() > peak {
                peak = y.abs();
                peak_idx = i;
            }
        }
        println!(
            "  pitch {cents:>7.0} cents: first output {:>6} samples ({:>5.1} ms), echo peak@{:>6} ({:>5.1} ms)",
            first.unwrap_or(0),
            first.unwrap_or(0) as f32 / fs * 1000.0,
            peak_idx,
            peak_idx as f32 / fs * 1000.0
        );
    }

    // ---- buffer size vs latency and quality ----
    println!("\n-- buffer size vs latency / quality (+1200 cents, 220 Hz in) --");
    measure_buffer::<2400>(fs, 220.0, 1200.0);
    measure_buffer::<1600>(fs, 220.0, 1200.0);
    measure_buffer::<1200>(fs, 220.0, 1200.0);
    measure_buffer::<800>(fs, 220.0, 1200.0);
    measure_buffer::<600>(fs, 220.0, 1200.0);

    // ---- FeedbackDelay: repeats climb ----
    println!("\n-- FeedbackDelay with shifter in the loop (pitch 1200 cents) --");
    {
        let mut d = FeedbackDelay::<48000, 2400>::new(fs, 300.0);
        d.set_pitch_cents(1200.0);
        d.set_pitch_mode(PitchMode::Target);
        d.set_pitch_glide_ms(0.0);
        d.delay_ms = 300.0;
        d.feedback = 0.6;
        d.mix = 1.0;
        let mut out = vec![0.0f32; 96_000];
        for i in 0..96_000 {
            let x = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / fs).sin();
            out[i] = d.process(x);
        }
        let s = &out[20_000..90_000];
        for k in 0..5 {
            let f = 220.0 * (2.0f32).powi(k);
            println!("  repeat {k}: {f:>7.1} Hz magnitude {:.6}", dft_mag(s, fs, f));
        }
    }
}
