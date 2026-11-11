//! Phase vocoder measurement harness.
//!
//! Measures the same things the delay-line shifter probe does, so the two can
//! be compared directly: latency, pitch accuracy, envelope preservation and
//! harmonic content.

use pedal_dsp::PitchVocoder;
use std::f32::consts::{PI, TAU};

fn dft_mag(s: &[f32], fs: f32, f: f32) -> f32 {
    let n = s.len();
    let mut re = 0.0f32;
    let mut im = 0.0f32;
    for (i, &x) in s.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * i as f32 / (n - 1) as f32).cos();
        let ph = -TAU * f * i as f32 / fs;
        re += w * x * ph.cos();
        im += w * x * ph.sin();
    }
    (re * re + im * im).sqrt() / n as f32
}

fn dominant_freq(s: &[f32], fs: f32, lo: f32, hi: f32) -> (f32, f32) {
    let n = s.len();
    let k_lo = ((lo / fs) * n as f32).ceil().max(1.0) as usize;
    let k_hi = ((hi / fs) * n as f32).floor().min((n / 2 - 1) as f32) as usize;
    let mut best = (0.0f32, -1.0f32);
    let mut mag = vec![0.0f32; n / 2];
    let win: Vec<f32> = (0..n)
        .map(|i| 0.5 - 0.5 * (TAU * i as f32 / (n - 1) as f32).cos())
        .collect();
    for k in k_lo..=k_hi {
        let mut re = 0.0f32;
        let mut im = 0.0f32;
        for (i, &x) in s.iter().enumerate() {
            let ph = -TAU * k as f32 * i as f32 / n as f32;
            let w = win[i] * x;
            re += w * ph.cos();
            im += w * ph.sin();
        }
        mag[k] = (re * re + im * im).sqrt();
        if mag[k] > best.1 {
            best = (k as f32 * fs / n as f32, mag[k]);
        }
    }
    if best.0 > 0.0 {
        let k = ((best.0 / fs) * n as f32).round() as usize;
        if k > k_lo && k < k_hi {
            let a = (mag[k - 1] + 1e-12).ln();
            let b = (mag[k] + 1e-12).ln();
            let c = (mag[k + 1] + 1e-12).ln();
            let denom = a - 2.0 * b + c;
            if denom.abs() > 1e-9 {
                let delta = (0.5 * (a - c) / denom).clamp(-1.0, 1.0);
                best.0 = (k as f32 + delta) * fs / n as f32;
            }
        }
    }
    best
}

fn sine(n: usize, fs: f32, f: f32) -> Vec<f32> {
    (0..n).map(|i| (TAU * f * i as f32 / fs).sin()).collect()
}

fn measure_latency<const N: usize>(fs: f32, cents: f32) -> (f32, f32, f32) {
    let mut pv = PitchVocoder::<N>::new(fs);
    pv.set_pitch_cents(cents);
    let total = 8000;
    let mut out = vec![0.0f32; total];
    for i in 0..total {
        out[i] = pv.process(if i == 4000 { 1.0 } else { 0.0 });
    }
    let start = 3900usize;
    let end = (4000 + N + 500).min(total);
    let mut peak = (0usize, 0.0f32);
    let mut cnum = 0.0f32;
    let mut cden = 0.0f32;
    for i in start..end {
        let a = out[i].abs();
        if a > peak.1 {
            peak = (i, a);
        }
        cnum += (i - 4000) as f32 * out[i].abs();
        cden += out[i].abs();
    }
    let centroid = if cden > 1e-9 { cnum / cden } else { 0.0 };
    (peak.0 as f32 - 4000.0, centroid, peak.1)
}

fn main() {
    let fs = 48_000.0f32;

    println!("=== latency (impulse response peak) ===");
    println!("  N     cents   peak(samples)  peak(ms)   peakmag");
    macro_rules! lat {
        ($n:literal) => {{
            for &cents in &[0.0f32, -1200.0, 1200.0] {
                let (pk, _cen, pm) = measure_latency::<$n>(fs, cents);
                println!(
                    "  {n:>5} {cents:>8.0} {pk:>13.0} {d:>9.2} {pm:>9.4}",
                    n = $n,
                    cents = cents,
                    pk = pk,
                    d = pk / fs * 1e3,
                    pm = pm
                );
            }
        }};
    }
    lat!(256);
    lat!(512);
    lat!(1024);

    println!();
    println!("=== pitch accuracy (N=1024) ===");
    println!("  in(Hz)   cents   out(Hz)   expect   err(cents)   rms");
    for &fin in &[80.0f32, 110.0, 220.0, 440.0, 880.0, 1760.0] {
        for &cents in &[0.0f32, -1200.0, 1200.0] {
            let mut pv = PitchVocoder::<1024>::new(fs);
            pv.set_pitch_cents(cents);
            let input = sine(40_000, fs, fin);
            let out: Vec<f32> = input.iter().map(|&x| pv.process(x)).collect();
            let expect = fin * 2.0f32.powf(cents / 1200.0);
            let lo = expect * 0.5;
            let hi = expect * 2.0;
            let (f, _) = dominant_freq(&out[20_000..40_000], fs, lo, hi);
            let err = 1200.0 * (f / expect).log2();
            let rms = (out[20_000..40_000].iter().map(|v| v * v).sum::<f32>() / 20_000.0).sqrt();
            println!(
                "  {fin:>7.1} {cents:>8.0} {f:>9.2} {expect:>8.2} {err:>+10.1} {:>8.4}",
                rms
            );
        }
    }

    println!();
    println!("=== envelope preservation (5 Hz AM, 220 Hz carrier, +700 cents) ===");
    {
        let mut pv = PitchVocoder::<1024>::new(fs);
        pv.set_pitch_cents(700.0);
        let n = 40_000usize;
        let out: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / fs;
                let env = 0.5 + 0.5 * (TAU * 5.0 * t).sin();
                pv.process(env * (TAU * 220.0 * t).sin())
            })
            .collect();
        let seg = &out[20_000..40_000];
        let a = TAU * 20.0 / fs;
        let mut lp = 0.0f32;
        let mut env = Vec::with_capacity(seg.len());
        for &x in seg {
            lp += a * (x.abs() - lp);
            env.push(lp);
        }
        let mean = env.iter().sum::<f32>() / env.len() as f32;
        for v in env.iter_mut() {
            *v -= mean;
        }
        let (f, _) = dominant_freq(&env, fs, 1.0, 20.0);
        println!("  envelope at {f:.3} Hz (expect 5.000)");
    }

    println!();
    println!("=== unity-path fidelity (220 Hz, 0 cents) ===");
    {
        let mut pv = PitchVocoder::<1024>::new(fs);
        let input = sine(40_000, fs, 220.0);
        let out: Vec<f32> = input.iter().map(|&x| pv.process(x)).collect();
        let mut best = (0isize, -1.0f32);
        for delay in -2000..=2000 {
            let mut c = 0.0f32;
            let mut n = 0usize;
            for i in 20_000..40_000 {
                let j = i as isize + delay;
                if j >= 0 && (j as usize) < input.len() {
                    c += out[i] * input[j as usize];
                    n += 1;
                }
            }
            if n > 0 {
                c /= n as f32;
            }
            if c > best.1 {
                best = (delay, c);
            }
        }
        println!(
            "  best delay {} samples ({:.2} ms), correlation {:.4}",
            best.0,
            best.0 as f32 / fs * 1e3,
            best.1
        );
    }

    println!();
    println!("=== harmonic content (220 Hz -> 440 Hz, N=1024) ===");
    {
        let mut pv = PitchVocoder::<1024>::new(fs);
        pv.set_pitch_cents(1200.0);
        let input = sine(40_000, fs, 220.0);
        let out: Vec<f32> = input.iter().map(|&x| pv.process(x)).collect();
        let seg = &out[20_000..40_000];
        let m = dft_mag(seg, fs, 440.0);
        for h in 2..=4 {
            let mh = dft_mag(seg, fs, 220.0 * h as f32);
            println!("  H{h} ({} Hz): {:>6.2} dB rel fundamental", 220.0 * h as f32, 20.0 * (mh / m).log10());
        }
    }

    println!();
    println!("=== N comparison (220 Hz -> 440 Hz) ===");
    println!("  N     latency(ms)  out(Hz)   err(cents)");
    macro_rules! run_n {
        ($n:literal) => {{
            let mut pv = PitchVocoder::<$n>::new(fs);
            pv.set_pitch_cents(1200.0);
            let input = sine(40_000, fs, 220.0);
            let out: Vec<f32> = input.iter().map(|&x| pv.process(x)).collect();
            let (f, _) = dominant_freq(&out[20_000..40_000], fs, 300.0, 700.0);
            let err = 1200.0 * (f / 440.0).log2();
            let (pk, _, _) = measure_latency::<$n>(fs, 1200.0);
            println!(
                "  {n:>5} {lat:>11.2} {f:>9.2} {err:>+10.1}",
                n = $n,
                lat = pk / fs * 1e3,
                f = f,
                err = err
            );
        }};
    }
    run_n!(256);
    run_n!(512);
    run_n!(1024);
}
