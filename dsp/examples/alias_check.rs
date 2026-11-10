//! Measure harmonic vs. non-harmonic (aliased) energy, and hard clipping.
//!
//! Usage: `cargo run --release --example alias_check`

use pedal_dsp::chain::EffectChain;
use pedal_dsp::waveshape::{Oversampling, Shaper, ShaperKind};

fn dft_mag(s: &[f32], fs: f32, f: f32) -> f32 {
    let mut re = 0.0f32;
    let mut im = 0.0f32;
    for (i, v) in s.iter().enumerate() {
        let w = 2.0 * std::f32::consts::PI * f * i as f32 / fs;
        re += v * w.cos();
        im += v * w.sin();
    }
    (re * re + im * im).sqrt() / s.len() as f32
}

/// Frequencies that are NOT multiples of f0 = 1000 Hz.
const PROBES: &[f32] = &[1234.0, 2345.0, 3456.0, 4567.0, 5678.0, 6789.0, 7890.0, 8901.0, 12345.0];

fn shaper_test(os: Oversampling, drive: f32) {
    let fs = 48_000.0f32;
    let f0 = 1_000.0f32;
    let n = 48_000;
    let mut s = Shaper::new(ShaperKind::Tanh);
    s.oversampling = os;
    s.set_drive(drive);
    s.set_level(0.8);
    s.set_mix(1.0);

    // A sawtooth has harmonics at every multiple of f0, so any energy at a
    // non-multiple frequency is intermodulation / aliasing.
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let phase = (i as f32 * f0 / fs).fract();
        let x = 2.0 * phase - 1.0;
        out.push(s.process(x));
    }
    let out = &out[1000..];
    let fund = dft_mag(out, fs, f0);
    let alias: f32 = PROBES.iter().map(|&f| dft_mag(out, fs, f)).sum();
    println!(
        "shaper tanh drive {drive:>4} os {os:?}: fund {fund:.6}, non-harmonic sum {alias:.6} ({:>5.1}% of fund)",
        100.0 * alias / fund
    );
}

fn chain_test(label: &str, drive: f32, level: f32, master: f32) {
    let fs = 48_000.0f32;
    let f0 = 1_000.0f32;
    let n = 48_000;
    let mut chain = EffectChain::new(fs);
    chain.drive = drive;
    chain.distortion_mix = 1.0;
    chain.distortion_level = level;
    chain.tone = 0.0;
    chain.level = master;
    chain.delay_mix = 0.0;
    chain.reverb_mix = 0.0;
    chain.update_params();

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let phase = (i as f32 * f0 / fs).fract();
        let x = 2.0 * phase - 1.0;
        let (l, _) = chain.process_stereo(x, x);
        out.push(l);
    }
    let clipped = out.iter().filter(|&&v| v.abs() >= 0.9999).count();
    let out = &out[1000..];
    let fund = dft_mag(out, fs, f0);
    let alias: f32 = PROBES.iter().map(|&f| dft_mag(out, fs, f)).sum();
    println!(
        "chain {label:>9}: drive {drive:>4} lvl {level:>3} master {master:>3} | fund {fund:.6}, alias {alias:.6} ({:>5.1}%), clipped samples {clipped:>6}/{n}",
        100.0 * alias / fund
    );
}

fn guitar_test(label: &str, drive: f32, level: f32, master: f32, fs: f32) {
    let path = std::path::Path::new("/Users/yoran/Downloads/3-Audio 0010 [2026-11-03 215835].wav");
    let mut r = match hound::WavReader::open(path) {
        Ok(r) => r,
        Err(e) => {
            println!("guitar_test: cannot open WAV: {e}");
            return;
        }
    };
    let spec = r.spec();
    let raw: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 24) => r
            .samples::<i32>()
            .map(|s| s.unwrap() as f32 / 8_388_608.0)
            .collect(),
        (hound::SampleFormat::Int, 16) => {
            r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
        }
        (hound::SampleFormat::Float, 32) => {
            r.samples::<f32>().map(|s| s.unwrap()).collect()
        }
        other => {
            println!("guitar_test: unsupported format {other:?}");
            return;
        }
    };
    // Average pairs to mono.
    let samples: Vec<f32> = raw.chunks(2).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect();
    let in_peak = samples.iter().fold(0.0f32, |a, &b| a.max(b.abs()));

    let mut chain = EffectChain::new(fs);
    chain.drive = drive;
    chain.distortion_mix = 1.0;
    chain.distortion_level = level;
    chain.tone = 0.0;
    chain.level = master;
    chain.delay_mix = 0.0;
    chain.reverb_mix = 0.0;
    chain.update_params();

    let mut clipped = 0usize;
    let mut out_peak = 0.0f32;
    let mut out = Vec::with_capacity(samples.len());
    for &x in &samples {
        let (l, _) = chain.process_stereo(x, x);
        if l.abs() >= 0.9999 {
            clipped += 1;
        }
        out_peak = out_peak.max(l.abs());
        out.push(l);
    }
    let n = out.len();
    let alias: f32 = PROBES.iter().map(|&f| dft_mag(&out, fs, f)).sum();
    println!(
        "guitar {label:>8} @{fs:>6.0}: drive {drive:>4} lvl {level:>3} master {master:>3} | in peak {in_peak:.3}, out peak {out_peak:.3}, clipped {clipped:>6}/{n} ({:.2}%), alias {alias:.6}",
        100.0 * clipped as f32 / n as f32
    );
}

/// Reproduce the live app's old behaviour: recreate the shaper (resetting the
/// oversampling FIRs) on every single sample.
fn live_simulation_test() {
    let fs = 48_000.0f32;
    let f0 = 1_000.0f32;
    let n = 48_000;
    let mut chain = EffectChain::new(fs);
    chain.drive = 3.0;
    chain.distortion_mix = 1.0;
    chain.distortion_level = 0.8;
    chain.tone = 0.0;
    chain.level = 0.9;
    chain.update_params();

    let mut out = Vec::with_capacity(n);
    // The live app calls apply_params -> set_shaper once per output callback,
    // and the callback is 512 frames.  Each reset zeroes the oversampling
    // FIRs, so every callback starts with a discontinuity.
    for i in 0..n {
        if i % 512 == 0 {
            chain.set_shaper(ShaperKind::Tanh);
        }
        let phase = (i as f32 * f0 / fs).fract();
        let x = 2.0 * phase - 1.0;
        let (l, _) = chain.process_stereo(x, x);
        out.push(l);
    }
    let rms = (out.iter().map(|v| v * v).sum::<f32>() / out.len() as f32).sqrt();
    let alias: f32 = PROBES.iter().map(|&f| dft_mag(&out, fs, f)).sum();
    let fund = dft_mag(&out, fs, f0);
    println!(
        "live simulation (reset every 512 samples): rms {rms:.4}, fund {fund:.6}, non-harmonic {alias:.6} ({:.1}% of fund)",
        100.0 * alias / fund
    );
}

/// Verify the delay time is actually honoured: fire an impulse and find the
/// first echo.  With feedback at zero there is exactly one echo.
fn delay_test() {
    let fs = 48_000.0f32;
    for ms in [50.0, 150.0, 300.0, 600.0, 900.0, 1200.0] {
        let mut chain = EffectChain::new(fs);
        chain.delay_ms = ms;
        chain.delay_mix = 1.0;
        chain.delay_feedback = 0.0;
        chain.distortion_mix = 0.0;
        chain.distortion_level = 1.0;
        chain.reverb_mix = 0.0;
        chain.level = 1.0;
        chain.update_params();

        let mut peak_idx = 0usize;
        let mut peak_val = 0.0f32;
        for i in 0..70000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, _) = chain.process_stereo(x, x);
            if i > 10 && l.abs() > peak_val {
                peak_val = l.abs();
                peak_idx = i;
            }
        }
        println!(
            "delay {:>5.0} ms -> first echo at {:>6} samples = {:>6.1} ms (peak {:.3})",
            ms,
            peak_idx,
            peak_idx as f32 / fs * 1000.0,
            peak_val
        );
    }
}

/// Verify the static pitch shift: a sine goes in, and the delay's output
/// should sit at the shifted frequency (not swing around it).
fn pitch_test() {
    let fs = 48_000.0f32;
    let f0 = 440.0f32;
    for cents in [0.0, 600.0, 1200.0, -1200.0] {
        let mut chain = EffectChain::new(fs);
        chain.delay_ms = 200.0;
        chain.delay_mix = 1.0;
        chain.delay_feedback = 0.0;
        chain.pitch_cents = cents;
        chain.distortion_mix = 0.0;
        chain.distortion_level = 1.0;
        chain.reverb_mix = 0.0;
        chain.level = 1.0;
        chain.update_params();

        let mut out = Vec::with_capacity(96_000);
        for i in 0..96_000 {
            let x = (2.0 * std::f32::consts::PI * f0 * i as f32 / fs).sin();
            out.push(chain.process_mono(x));
        }

        fn zc_freq(s: &[f32], fs: f32) -> f32 {
            let mut n = 0usize;
            for i in 1..s.len() {
                if s[i - 1] < 0.0 && s[i] >= 0.0 {
                    n += 1;
                }
            }
            n as f32 / (s.len() as f32 / fs)
        }
        let f = zc_freq(&out[20_000..90_000], fs);
        let expect = f0 * (2.0f32).powf(cents / 1200.0);
        let rms = (out[20_000..90_000].iter().map(|v| v * v).sum::<f32>() / 70_000.0).sqrt();
        println!(
            "pitch {cents:>7.0} cents -> output {f:>7.1} Hz (expected {expect:>7.1}), rms {rms:.3}"
        );
    }

    // With feedback the shift accumulates: each repeat is shifted again, so
    // the output should contain 440, 880, 1760, 3520 ... Hz.
    let mut chain = EffectChain::new(fs);
    chain.delay_ms = 300.0;
    chain.delay_mix = 1.0;
    chain.delay_feedback = 0.6;
    chain.pitch_cents = 1200.0;
    chain.distortion_mix = 0.0;
    chain.distortion_level = 1.0;
    chain.reverb_mix = 0.0;
    chain.level = 1.0;
    chain.update_params();
    let mut out = Vec::new();
    for i in 0..96_000 {
        let x = (2.0 * std::f32::consts::PI * f0 * i as f32 / fs).sin();
        out.push(chain.process_mono(x));
    }
    let s = &out[20_000..90_000];
    for k in 0..5 {
        let f = f0 * (2.0f32).powi(k);
        println!(
            "  repeat {k}: {f:>7.1} Hz bin magnitude {:.6}",
            dft_mag(s, fs, f)
        );
    }
}

/// 0 ms should be a one-sample comb: finite, bounded, and it should ring.
fn zero_delay_test() {
    let fs = 48_000.0f32;
    let mut chain = EffectChain::new(fs);
    chain.delay_ms = 0.0;
    chain.delay_mix = 1.0;
    chain.delay_feedback = 0.9;
    chain.distortion_mix = 0.0;
    chain.distortion_level = 1.0;
    chain.reverb_mix = 0.0;
    chain.level = 1.0;
    chain.update_params();
    let mut peak = 0.0f32;
    let mut finite = true;
    let mut last = 0.0f32;
    for i in 0..48_000 {
        let x = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / fs).sin();
        let y = chain.process_mono(x);
        if !y.is_finite() {
            finite = false;
        }
        peak = peak.max(y.abs());
        if i > 40_000 {
            last = y.abs();
        }
    }
    println!(
        "0 ms, feedback 0.9: peak {peak:.3}, still ringing at 1 s ({last:.4}), finite {finite}"
    );
}

/// Verify the glide: jump the pitch target and watch the output frequency
/// ramp from 440 Hz to 880 Hz.
fn glide_test() {
    let fs = 48_000.0f32;
    let f0 = 440.0f32;
    fn zc_freq(s: &[f32], fs: f32) -> f32 {
        let mut n = 0usize;
        for i in 1..s.len() {
            if s[i - 1] < 0.0 && s[i] >= 0.0 {
                n += 1;
            }
        }
        n as f32 / (s.len() as f32 / fs)
    }
    for glide_ms in [0.0, 500.0, 2000.0] {
        let mut chain = EffectChain::new(fs);
        chain.delay_ms = 50.0;
        chain.delay_mix = 1.0;
        chain.delay_feedback = 0.0;
        chain.pitch_cents = 0.0;
        chain.pitch_glide_ms = glide_ms;
        chain.distortion_mix = 0.0;
        chain.distortion_level = 1.0;
        chain.reverb_mix = 0.0;
        chain.level = 1.0;
        chain.update_params();

        for i in 0..2000 {
            let x = (2.0 * std::f32::consts::PI * f0 * i as f32 / fs).sin();
            chain.process_mono(x);
        }
        chain.pitch_cents = 1200.0;
        chain.update_params();

        let mut out = Vec::new();
        for i in 0..96_000 {
            let x = (2.0 * std::f32::consts::PI * f0 * (2000 + i) as f32 / fs).sin();
            out.push(chain.process_mono(x));
        }
        print!("glide {glide_ms:>5.0} ms: ");
        for w in [2_000usize, 8_000, 20_000, 44_000, 92_000] {
            let f = zc_freq(&out[w..w + 2000], fs);
            print!("@{:>4} ms {:.0} Hz   ", w * 1000 / fs as usize, f);
        }
        println!();
    }
}

/// Verify the continuous sweep rate: the output pitch should keep climbing.
fn rate_test() {
    let fs = 48_000.0f32;
    let f0 = 440.0f32;
    fn zc_freq(s: &[f32], fs: f32) -> f32 {
        let mut n = 0usize;
        for i in 1..s.len() {
            if s[i - 1] < 0.0 && s[i] >= 0.0 {
                n += 1;
            }
        }
        n as f32 / (s.len() as f32 / fs)
    }
    for rate in [0.0, 300.0, 1200.0] {
        let mut chain = EffectChain::new(fs);
        chain.delay_ms = 100.0;
        chain.delay_mix = 1.0;
        chain.delay_feedback = 0.0;
        chain.pitch_cents = 0.0;
        chain.pitch_rate = rate;
        chain.pitch_mode = pedal_dsp::PitchMode::Sweep;
        chain.distortion_mix = 0.0;
        chain.distortion_level = 1.0;
        chain.reverb_mix = 0.0;
        chain.level = 1.0;
        chain.update_params();

        let mut out = Vec::new();
        for i in 0..96_000 {
            let x = (2.0 * std::f32::consts::PI * f0 * i as f32 / fs).sin();
            out.push(chain.process_mono(x));
        }
        print!("rate {rate:>6.0} cents/s: ");
        for w in [2_000usize, 20_000, 40_000, 60_000, 80_000] {
            let f = zc_freq(&out[w..w + 2000], fs);
            print!("@{:>4} ms {:.0} Hz  ", w * 1000 / fs as usize, f);
        }
        println!();
    }

    // Same rate, Target mode: the sweep must be ignored and the pitch hold.
    let mut chain = EffectChain::new(fs);
    chain.delay_ms = 100.0;
    chain.delay_mix = 1.0;
    chain.delay_feedback = 0.0;
    chain.pitch_cents = 0.0;
    chain.pitch_rate = 1200.0;
    chain.pitch_mode = pedal_dsp::PitchMode::Target;
    chain.distortion_mix = 0.0;
    chain.distortion_level = 1.0;
    chain.reverb_mix = 0.0;
    chain.level = 1.0;
    chain.update_params();
    let mut out = Vec::new();
    for i in 0..96_000 {
        let x = (2.0 * std::f32::consts::PI * f0 * i as f32 / fs).sin();
        out.push(chain.process_mono(x));
    }
    let f = zc_freq(&out[80_000..90_000], fs);
    println!("Target mode, rate 1200 c/s: {f:.0} Hz at 1.8 s (should hold at 432 Hz)");
}

fn main() {
    println!("== pitch rate ==");
    rate_test();
    println!("\n== pitch glide ==");
    glide_test();
    println!("\n== zero-delay check ==");
    zero_delay_test();
    println!("\n== pitch modulation ==");
    pitch_test();
    println!("\n== delay time ==");
    delay_test();
    println!("\n== root cause: per-sample shaper reset ==");
    live_simulation_test();
    println!("\n== shaper oversampling (sawtooth) ==");
    for drive in [3.0, 20.0] {
        shaper_test(Oversampling::None, drive);
        shaper_test(Oversampling::X2, drive);
        shaper_test(Oversampling::X4, drive);
    }
    println!("\n== full chain (sawtooth) ==");
    chain_test("defaults", 3.0, 0.8, 0.9);
    chain_test("cranked", 20.0, 2.0, 2.0);
    println!("\n== real guitar recording ==");
    guitar_test("defaults", 3.0, 0.8, 0.9, 48_000.0);
    guitar_test("defaults", 3.0, 0.8, 0.9, 44_100.0);
    guitar_test("cranked", 20.0, 2.0, 2.0, 48_000.0);
    guitar_test("cranked", 20.0, 2.0, 2.0, 44_100.0);
}
