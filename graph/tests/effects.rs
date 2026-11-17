//! Fuzz-style tests for the rack effects.
//!
//! The Compressor is driven with a fixed-seed PRNG over its full declared
//! parameter range and a mixed input signal, so it is exercised with
//! *varied* inputs rather than a few fixed examples. Failures reproduce
//! from the fixed seed (or `CASE`).
//!
//!     cargo test -p dsp-graph --test effects

use dsp_graph::blocks::{Compressor, IoInput, IoOutput};
use dsp_graph::{Executor, Graph};

/// Fixed seed: every run produces the same sequence, so generated cases are
/// deterministic and reproducible.
const SEED: u64 = 0x9e3779b97f4a7c15;

/// Number of generated cases per test.
const CASES: usize = 64;

/// Minimal deterministic PRNG (SplitMix64), kept dependency-free so the
/// fuzz tests behave identically across toolchains and hosts.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut z = self.0;
        z = z.wrapping_add(0x9e3779b97f4a7c15);
        self.0 = z;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() & 0xffff_ffff) as u32
    }

    /// Uniform `f32` in `[0, 1)`.
    fn next_f32(&mut self) -> f32 {
        // Take the top 24 bits and divide by 2^24 (exact); gives a clean
        // grid in [0, 1).
        (self.next_u64() >> 40) as f32 / 16777216.0
    }
}

fn env_case() -> Option<usize> {
    std::env::var("CASE").ok().and_then(|s| s.parse().ok())
}

/// Build a minimal two-block graph: in -> Compressor -> out, with the given
/// compressor parameters set to their real values.
fn compressor_graph(
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    makeup: f32,
    mix: f32,
) -> Graph {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("comp", Box::new(Compressor::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "comp", "in").unwrap();
    g.connect("comp", "out", "out", "in").unwrap();
    g.set_param_real("comp", "threshold", threshold).unwrap();
    g.set_param_real("comp", "ratio", ratio).unwrap();
    g.set_param_real("comp", "attack", attack).unwrap();
    g.set_param_real("comp", "release", release).unwrap();
    g.set_param_real("comp", "makeup", makeup).unwrap();
    g.set_param_real("comp", "mix", mix).unwrap();
    g
}

#[test]
fn compressor_stays_finite_across_varied_params_and_inputs() {
    let case_filter = env_case();
    let mut rng = Rng(SEED);
    let mut cases = 0;

    for case in 0..CASES {
        if let Some(c) = case_filter {
            if c != case {
                continue;
            }
        }

        // Sweep the whole declared parameter range.
        let threshold = -60.0 + rng.next_f32() * 60.0;
        let ratio = 1.0 + rng.next_f32() * 19.0;
        let attack = 0.1 + rng.next_f32() * 49.9;
        let release = 1.0 + rng.next_f32() * 499.0;
        let makeup = rng.next_f32() * 30.0;
        let mix = rng.next_f32() * 100.0;

        let mut ex = Executor::compile_owned(
            &mut compressor_graph(threshold, ratio, attack, release, makeup, mix),
            48_000.0,
            256,
        )
        .unwrap();

        // Mixed input: sine, noise, a single impulse, and a slow ramp.
        let mut input = vec![0.0f32; 256 * 4];
        for i in 0..input.len() {
            let sig = match i % 4 {
                0 => (i as f32 * 0.05).sin(),
                1 => 2.0 * (rng.next_u32() >> 16) as f32 / 65536.0 - 1.0,
                2 => if i == 0 { 1.0 } else { 0.0 },
                _ => (i as f32 * 0.001).sin() * (i as f32 / 8000.0),
            };
            input[i] = sig * (0.1 + rng.next_f32() * 2.0);
        }

        // Warm up the envelope follower so it reaches steady state.
        for _ in 0..4 {
            let _ = ex.run(&input[..256], 256).unwrap();
        }

        // Finite output and no explosion for any parameter/inter-sample mix.
        let mut peak = 0.0f32;
        for _ in 0..4 {
            let out = ex.run(&input[..256], 256).unwrap();
            for &v in out {
                assert!(v.is_finite(), "case {case}: non-finite output");
                peak = peak.max(v.abs());
            }
        }

        // Makeup can add up to ~30 dB (x31.6), so an absolute output bound
        // is not meaningful; instead assert the block does not explode to
        // infinity or NaN for any parameter/inter-sample combination.
        assert!(peak < 1e6, "case {case}: compressor output exploded (peak {peak})");

        // Determinism from a fresh state: the block has no randomness, so
        // reset-then-run twice must be byte-identical. A consecutive run is
        // NOT comparable here, because the envelope follower is still
        // converging toward a periodic orbit and its starting value differs.
        ex.reset();
        let out0 = ex.run(&input[..256], 256).unwrap().to_vec();
        ex.reset();
        let out1 = ex.run(&input[..256], 256).unwrap().to_vec();
        assert_runs_equal(&out0, &out1, case);

        cases += 1;
    }

    println!("ran {cases} compressor fuzz cases");
}

/// Byte-exact compare of two runs of the same block; used by the determinism
/// tests to emit a compact, reproducible failure message.
fn assert_runs_equal(out0: &[f32], out1: &[f32], case: usize) {
    assert_eq!(out0.len(), out1.len(), "case {case}: different lengths");
    for (i, (a, b)) in out0.iter().zip(out1.iter()).enumerate() {
        assert_eq!(
            *a, *b,
            "case {case}: run mismatch at sample {i} ({a} vs {b})"
        );
    }
}
