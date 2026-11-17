//! Fuzz-style tests for the [`EffectChain`].
//!
//! The effect chain is the pedal's effects implementation: distortion
//! (waveshaper), tone stack, delay, reverb and the master loop. This test
//! does not hand-pick a table of inputs; a fixed-seed PRNG generates a wide
//! spread of parameter sets and input signals, so the chain is exercised
//! with *varied* inputs rather than a few fixed examples.
//!
//! The seed is fixed, so any failure reproduces by rerunning with the same
//! seed (or by setting `CASE` to a single case index for debugging).
//!
//! Run with the project's established Rust test command:
//!
//!     cargo test -p pedal-dsp
//!
//! See [`EffectChain`] for where the effect chain lives.

use pedal_dsp::EffectChain;

/// Fixed seed: every run produces the same sequence, so generated cases are
/// deterministic and reproducible.
const SEED: u64 = 0x9e3779b97f4a7c15;

/// Number of generated cases per test.
const CASES: usize = 128;

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

    /// Uniform `f32` in `[lo, hi)`.
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }
}

/// A signal family, drawn from a menu so no single case dominates.
enum Input {
    Sine,
    Noise,
    Impulse,
    Ramp,
}

impl Input {
    fn next(rng: &mut Rng) -> Self {
        match rng.next_u32() % 4 {
            0 => Input::Sine,
            1 => Input::Noise,
            2 => Input::Impulse,
            _ => Input::Ramp,
        }
    }

    fn sample(&self, t: usize, amplitude: f32, rng: &mut Rng) -> f32 {
        let raw = match self {
            Input::Sine => (t as f32 * 0.05).sin(),
            Input::Noise => 2.0 * (rng.next_u32() >> 16) as f32 / 65536.0 - 1.0,
            Input::Impulse => if t == 0 { 1.0 } else { 0.0 },
            Input::Ramp => {
                // Slow ramp-like kick; bounded so it never overflows the
                // chain's soft limiter.
                (t as f32 * 0.001).sin() * (t as f32 / 8000.0)
            }
        };
        raw * amplitude
    }
}

/// Generate one fuzz case: a parameter set plus a short input buffer.
///
/// The amplitude is log-uniform so quiet and hot inputs are equally likely.
fn generate_case(rng: &mut Rng) -> (Vec<f32>, f32) {
    // Log-uniform amplitude: spread across ~9 decades.
    let amplitude = 1e-3f32 * (1e6f32 / 1e-3f32).powf(rng.next_f32());

    let n = 256;
    let mut input = Vec::with_capacity(n);
    for t in 0..n {
        let sig = Input::next(rng);
        input.push(sig.sample(t, amplitude, rng));
    }
    (input, amplitude)
}

/// Push one case's parameter set through the chain.
fn fuzz_params(rng: &mut Rng, chain: &mut EffectChain) {
    chain.drive = rng.range(0.1, 20.0);
    chain.distortion_level = rng.range(0.0, 4.0);
    chain.distortion_mix = rng.range(0.0, 1.0);
    chain.tone = rng.range(-1.0, 1.0);
    chain.level = rng.range(0.0, 2.0);
    chain.delay_ms = rng.range(10.0, 800.0);
    chain.delay_mix = rng.range(0.0, 1.0);
    chain.delay_feedback = rng.range(0.0, 0.95);
    chain.pitch_cents = rng.range(-2400.0, 2400.0);
    chain.pitch_glide_ms = rng.range(0.0, 200.0);
    chain.pitch_rate = rng.range(-20.0, 20.0) * 100.0;
    chain.pitch_mode = match rng.next_u32() % 3 {
        0 => pedal_dsp::PitchMode::Target,
        1 => pedal_dsp::PitchMode::Sweep,
        _ => pedal_dsp::PitchMode::Shepard,
    };
    chain.sweep_gate = rng.next_u32() % 2 == 0;
    chain.jitter = rng.range(0.0, 1.0);
    chain.reverb_mix = rng.range(0.0, 1.0);
    chain.enabled = rng.next_u32() % 2 == 0;
}

/// Run `input` through a *fresh* EffectChain whose parameters are fixed by
/// `rng` (seeded identically each call). Because the seed and input are the
/// same, this always produces the identical output — it is the building
/// block for the determinism and reset-idempotency checks.
fn chain_output(rng: &mut Rng, input: &[f32]) -> Vec<f32> {
    let mut chain = EffectChain::new(pedal_dsp::DEFAULT_SAMPLE_RATE);
    fuzz_params(rng, &mut chain);
    chain.update_params();
    input.iter().map(|&s| chain.process_mono(s)).collect()
}

fn env_case() -> Option<usize> {
    std::env::var("CASE").ok().and_then(|s| s.parse().ok())
}

#[test]
fn chain_stays_finite_and_bounded_across_varied_inputs() {
    let case_filter = env_case();
    let mut rng = Rng(SEED);
    let mut cases = 0usize;

    for case in 0..CASES {
        if let Some(c) = case_filter {
            if c != case {
                continue;
            }
        }

        let (input, _) = generate_case(&mut rng);
        let mut chain = EffectChain::new(pedal_dsp::DEFAULT_SAMPLE_RATE);
        fuzz_params(&mut rng, &mut chain);
        chain.update_params();

        let mut peak = 0.0f32;
        for &s in &input {
            let y = chain.process_mono(s);
            assert!(
                y.is_finite(),
                "case {case}: non-finite output from input {s}"
            );
            peak = peak.max(y.abs());
        }

        // The chain ends in soft_limit(), which clamps to [-1, 1] before the
        // cubic, so every output is bounded no matter the parameters or the
        // input amplitude.
        assert!(
            peak <= 1.0 + 1e-6,
            "case {case}: output exceeded soft_limit ({peak})"
        );
        cases += 1;
    }

    println!("ran {cases} generated EffectChain cases");
}

#[test]
fn chain_is_deterministic_for_a_fixed_seed() {
    let case_filter = env_case();
    // `rng` drives input generation only; the chains are each re-seeded to
    // the fixed seed so they always configure identically.
    let mut rng = Rng(SEED);
    let mut cases = 0usize;

    for case in 0..CASES {
        if let Some(c) = case_filter {
            if c != case {
                continue;
            }
        }

        let (input, _) = generate_case(&mut rng);
        let out_a = chain_output(&mut Rng(SEED), &input);
        let out_b = chain_output(&mut Rng(SEED), &input);

        assert_eq!(out_a, out_b, "case {case}: non-deterministic chain");
        cases += 1;
    }

    println!("ran {cases} determinism checks");
}

#[test]
fn chain_reset_is_idempotent() {
    let case_filter = env_case();
    let mut rng = Rng(SEED);
    let mut cases = 0usize;

    for case in 0..CASES {
        if let Some(c) = case_filter {
            if c != case {
                continue;
            }
        }

        let (input, _) = generate_case(&mut rng);

        // A freshly constructed chain and one that has been reset both begin
        // with an empty (cleared) delay line, so a reset must leave the
        // chain in an equivalent state: the pristine run and the reset run
        // over the same input must produce identical output.
        let out_pristine = chain_output(&mut Rng(SEED), &input);
        let out_reset: Vec<f32> = {
            let mut chain = EffectChain::new(pedal_dsp::DEFAULT_SAMPLE_RATE);
            fuzz_params(&mut Rng(SEED), &mut chain);
            chain.update_params();
            chain.reset();
            input.iter().map(|&s| chain.process_mono(s)).collect()
        };

        assert_eq!(
            out_pristine, out_reset,
            "case {case}: reset changed the output"
        );
        cases += 1;
    }

    println!("ran {cases} reset checks");
}
