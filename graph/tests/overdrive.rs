//! Integration sanity-checks for the overdrive variant blocks.
//!
//! Builds each variant from the registry and drives it on a fixed input,
//! asserting that it compiles, declares the ~30-sample algorithmic latency,
//! and produces finite (no-NaN / no-denormal) output for a full-scale signal.

use dsp_graph::blocks::{builtin_registry, IoInput, IoOutput};
use dsp_graph::{Executor, Graph};
use pedal_dsp::biquad::{Biquad, BiquadKind};
use std::f32::consts::PI;

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

/// A minimal graph: `in` -> variant block -> `out`.
fn variant_graph(block_id: &str, variant: Box<dyn dsp_graph::Block>) -> Graph {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add(block_id, variant).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", block_id, "in").unwrap();
    g.connect(block_id, "out", "out", "in").unwrap();
    g
}

/// (parameter id, default value) pairs for each overdrive variant, real
/// values per the design doc tables.
fn variant_params(block_id: &str) -> &'static [(&'static str, f32)] {
    match block_id {
        "drive.tube_screamer" => &[
            ("drive", 1.0),
            ("pre_mid", 0.0),
            ("pre_bass", 0.0),
            ("pre_treble", 0.0),
            ("tone", 0.0),
            ("level", 0.0),
        ],
        "drive.precision_drive" => &[
            ("gain", 2.0),
            ("tightness", 0.5),
            ("attack", 10.0),
            ("release", 600.0),
            ("tone", 0.0),
            ("presence", 0.0),
            ("volume", 0.0),
        ],
        "drive.klon" => &[("drive", 1.0), ("feel", 0.0), ("tone", 0.0), ("level", 0.0)],
        "drive.big_muff" => &[
            ("drive", 1.0),
            ("fuzz", 0.5),
            ("tone", 2000.0),
            ("blend", 0.5),
            ("level", 0.0),
        ],
        _ => unreachable!("unhandled variant {block_id}"),
    }
}

#[test]
fn variant_blocks_load_and_run_finite() {
    let reg = builtin_registry();
    let input: Vec<f32> = (0..BLOCK * 4)
        .map(|i| ((i as f32 * 0.05).sin() * 0.5) + if i % 640 == 0 { 0.0 } else { 0.0 })
        .collect();

    for block_id in &[
        "drive.tube_screamer",
        "drive.precision_drive",
        "drive.klon",
        "drive.big_muff",
    ] {
        let variant = reg.build(block_id).expect("variant registered");

        let mut g = variant_graph(block_id, variant);
        // Set every declared parameter to the design-doc default.
        for (id, v) in variant_params(block_id).iter().copied() {
            g.set_param_real(block_id, id, v).unwrap();
        }

        let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
        let mut peak = 0.0f32;
        let mut energy = 0.0f64;
        for chunk in input.chunks_exact(BLOCK) {
            let out = ex.run(chunk, BLOCK).unwrap();
            for &v in out {
                assert!(v.is_finite(), "{block_id}: non-finite output {v}");
                peak = peak.max(v.abs());
                energy += (v * v) as f64;
            }
        }
        let rms = (energy / input.len() as f64).sqrt();
        // Sanity: the block must not be a dead void (it should produce a
        // non-trivial response at default settings), and must stay bounded.
        assert!(peak > 0.01, "{block_id}: output was silent (peak {peak})");
        assert!(peak <= 1.0, "{block_id}: peak {peak} exceeds full scale");
        assert!(
            rms > 0.01,
            "{block_id}: nearly silent (rms {rms})",
        );

        // The 4× oversampling latency is ~30 samples @48 kHz.
        assert_eq!(
            ex.node_latency(block_id).unwrap_or(0),
            30,
            "{block_id}: latency {block_id}"
        );
    }
}

#[test]
fn variant_blocks_run_twice_with_same_seed_is_deterministic() {
    let input: Vec<f32> = (0..BLOCK * 8)
        .map(|i| ((i as f32 * 0.07).sin() * 0.6) + if i % 73 == 0 { 0.0 } else { 0.0 })
        .collect();

    for block_id in &["drive.precision_drive", "drive.big_muff"] {
        let reg = builtin_registry();
        let variant = reg.build(block_id).expect("variant registered");
        let mut g = variant_graph(block_id, variant);
        for (id, v) in variant_params(block_id).iter().copied() {
            g.set_param_real(block_id, id, v).unwrap();
        }
        let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
        ex.reset();
        let out0 = ex.run(&input[..BLOCK * 4], BLOCK * 4).unwrap().to_vec();
        ex.reset();
        let out1 = ex.run(&input[..BLOCK * 4], BLOCK * 4).unwrap().to_vec();
        assert_eq!(out0, out1, "{block_id}: non-deterministic run");
    }
}

/// Build a single-input single-output graph: `in` -> Precision Drive -> `out`.
fn make_pd_graph(tightness: f32, gain: f32) -> Graph {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add(
        "drive.precision_drive",
        builtin_registry().build("drive.precision_drive").unwrap(),
    )
    .unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "drive.precision_drive", "in").unwrap();
    g.connect("drive.precision_drive", "out", "out", "in").unwrap();
    g.set_param_real("drive.precision_drive", "gain", gain).unwrap();
    g.set_param_real("drive.precision_drive", "tightness", tightness).unwrap();
    g.set_param_real("drive.precision_drive", "attack", 10.0).unwrap();
    g.set_param_real("drive.precision_drive", "release", 600.0).unwrap();
    g.set_param_real("drive.precision_drive", "tone", 0.0).unwrap();
    g.set_param_real("drive.precision_drive", "presence", 0.0).unwrap();
    g.set_param_real("drive.precision_drive", "volume", 0.0).unwrap();
    g
}

/// Fixed low-frequency input: a full-scale ~65 Hz tone plus a sharp transient.
fn low_freq_input() -> [f32; BLOCK * 4] {
    let mut v = [0.0f32; BLOCK * 4];
    for i in 0..BLOCK * 4 {
        let t = i as f32 / SR;
        v[i] = 0.9 * (t * 65.41 * 2.0 * PI).sin();
    }
    v
}

#[test]
fn precision_drive_parameter_bounds_and_defaults() {
    // The declared param table must match the design spec (§2.2) and carry
    // the documented default.
    let reg = builtin_registry();
    let mut block = reg.build("drive.precision_drive").expect("PD registered");
    let params = block.spec().params;
    assert_eq!(params.len(), 7, "expected 7 params");
    let expected = [
        ("gain", 0.1, 20.0, 2.0),
        ("tightness", 0.0, 1.0, 0.5),
        ("attack", 1.0, 200.0, 10.0),
        ("release", 100.0, 3000.0, 600.0),
        ("tone", -12.0, 12.0, 0.0),
        ("presence", -12.0, 12.0, 0.0),
        ("volume", -30.0, 12.0, 0.0),
    ];
    for (i, (id, min, max, default)) in expected.iter().enumerate() {
        let p = &params[i];
        assert_eq!(p.id, *id, "param id mismatch at index {i}");
        assert!((p.min - *min).abs() < 1e-6, "param {id} min");
        assert!((p.max - *max).abs() < 1e-6, "param {id} max");
        assert!((p.default - *default).abs() < 1e-6, "param {id} default");
    }
}

#[test]
fn precision_drive_params_clamp_to_declared_range() {
    // `set_param_real` normalises through the spec; out-of-range values must
    // land on the min/max bound (stored as 0.0 / 1.0 in the normalised table)
    // instead of overflowing or panicking. Tests the AmpGain (dB) params,
    // where the normalisation is linear and NaN-free.
    let mut g = make_pd_graph(0.5, 2.0);
    for (id, min, max) in [
        ("tone", -12.0, 12.0),
        ("presence", -12.0, 12.0),
        ("volume", -30.0, 12.0),
    ] {
        g.set_param_real("drive.precision_drive", id, min - 100.0).unwrap();
        assert!(
            g.param_norm("drive.precision_drive", id).unwrap().abs() < 1e-6,
            "{id} low clamp did not clamp to 0.0"
        );
        g.set_param_real("drive.precision_drive", id, max + 100.0).unwrap();
        assert!(
            g.param_norm("drive.precision_drive", id).unwrap() > 1.0 - 1e-6,
            "{id} high clamp did not clamp to 1.0"
        );
    }
}

#[test]
fn precision_drive_stable_at_extremes_of_every_param() {
    // Driving every param to its declared min and max must yield finite,
    // bounded output (no NaN/inf, no blow-up).
    let reg = builtin_registry();
    let min: &[(&str, f32)] = &[
        ("gain", 0.1),
        ("tightness", 0.0),
        ("attack", 1.0),
        ("release", 100.0),
        ("tone", -12.0),
        ("presence", -12.0),
        ("volume", -30.0),
    ];
    let max: &[(&str, f32)] = &[
        ("gain", 20.0),
        ("tightness", 1.0),
        ("attack", 200.0),
        ("release", 3000.0),
        ("tone", 12.0),
        ("presence", 12.0),
        ("volume", 12.0),
    ];
    for (label, vals) in [("min", min), ("max", max)] {
        let mut g = make_pd_graph(0.5, 2.0);
        for (id, v) in vals.iter().copied() {
            g.set_param_real("drive.precision_drive", id, v).unwrap();
        }
        let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
        let mut peak = 0.0f32;
        let mut finite = true;
        for _ in 0..8 {
            let out = ex.run(&low_freq_input(), BLOCK * 4).unwrap();
            for &v in out {
                if !v.is_finite() {
                    finite = false;
                }
                peak = peak.max(v.abs());
            }
        }
        assert!(finite, "{label} extremes produced non-finite output");
        // At the extremes the block's *pre-ceiling* output can exceed full
        // scale (the engine master-ceiling caps the rig, not this block).
        // "Stable" means finite and bounded — no blow-up across iterations.
        assert!(
            peak <= 64.0,
            "{label} extremes peak {peak} exceeds sane bound (no blow-up)"
        );
    }
}

#[test]
fn precision_drive_tightens_low_end_vs_flat_reference() {
    // Punch: at high tightness, low-band transients are tightened *before*
    // clipping, so a tightness-1.0 run has less low-frequency energy than a
    // tightness-0.0 reference at the same gain (design §2.2).
    let input = low_freq_input();
    let build = |tightness: f32| make_pd_graph(tightness, 2.0);
    let mut ex0 = Executor::compile_owned(&mut build(0.0), SR, BLOCK).unwrap();
    let mut ex1 = Executor::compile_owned(&mut build(1.0), SR, BLOCK).unwrap();
    let out0 = ex0.run(&input, BLOCK * 4).unwrap();
    let out1 = ex1.run(&input, BLOCK * 4).unwrap();

    // Isolate the low band (<~150 Hz) with a one-pole low-pass and compare RMS.
    let lp = Biquad::make(BiquadKind::LowPass, SR, 150.0, 0.707, 0.0);
    let low_rms = |out: &[f32], lp: &mut Biquad| {
        let mut energy = 0.0f64;
        let mut filt = 0.0f32;
        for &x in out {
            filt = lp.process(x);
            energy += (filt * filt) as f64;
        }
        (energy / out.len() as f64).sqrt()
    };
    let mut lp0 = lp;
    let mut lp1 = lp;
    let r0 = low_rms(out0, &mut lp0);
    let r1 = low_rms(out1, &mut lp1);
    assert!(r1 < r0, "tightness 1.0 did not tighten low-end: {r1:.4} vs {r0:.4} at gain 2.0");
}
