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

fn main() {
    println!("== root cause: per-sample shaper reset ==");
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
