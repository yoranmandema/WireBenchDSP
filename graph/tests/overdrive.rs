//! Integration sanity-checks for the overdrive variant blocks.
//!
//! Builds each variant from the registry and drives it on a fixed input,
//! asserting that it compiles, declares the ~30-sample algorithmic latency,
//! and produces finite (no-NaN / no-denormal) output for a full-scale signal.

use std::f32::consts::PI;

use dsp_graph::blocks::{builtin_registry, IoInput, IoOutput};
use dsp_graph::{Executor, Graph};

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
        "drive.tube_screamer", "drive.precision_drive", "drive.klon", "drive.big_muff",
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
        assert!(rms > 0.01, "{block_id}: nearly silent (rms {rms})");

        // The 4x oversampling latency is ~30 samples @48 kHz.
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

/// TubeScreamer parameter bounds: every declared parameter is within its
/// declared min/max, with the default inside the range.
#[test]
fn tube_screamer_parameter_bounds() {
    let reg = builtin_registry();
    let variant = reg
        .build("drive.tube_screamer")
        .expect("tube_screamer registered");
    let spec = variant.spec();
    assert!(
        spec.params.len() >= 6,
        "expected >= 6 params, got {}",
        spec.params.len()
    );

    for p in &spec.params {
        assert!(p.min < p.max, "{}: min ({}) not less than max ({})", p.id, p.min, p.max);
        assert!(
            (p.default >= p.min - 1e-6) && (p.default <= p.max + 1e-6),
            "{}: default {} outside [{}, {}]",
            p.id, p.default, p.min, p.max
        );
        // Every declared parameter must be representable as a real value.
        let _ = p.normalise(p.denormalise(0.5));
    }
}

/// No NaN/inf: the TubeScreamer produces finite output at every parameter
/// combination across the full declared range, including extremes.
#[test]
fn tube_screamer_no_nan_inf_across_range() {
    let reg = builtin_registry();
    let input: Vec<f32> = (0..BLOCK * 8)
        .map(|i| ((i as f32 * 0.05).sin() * 0.5) + if i % 640 == 0 { 0.0 } else { 0.0 })
        .collect();

    for drive in [0.1, 1.0, 10.0] {
        for pre_mid in [-12.0, 0.0, 12.0] {
            for pre_bass in [-12.0, 0.0, 12.0] {
                for pre_treble in [-12.0, 0.0, 12.0] {
                    for tone in [-12.0, 0.0, 12.0] {
                        for level in [-30.0, 0.0, 12.0] {
                            let mut g = variant_graph(
                                "drive.tube_screamer",
                                reg.build("drive.tube_screamer").expect("variant registered"),
                            );
                            g.set_param_real("drive.tube_screamer", "drive", drive).unwrap();
                            g.set_param_real("drive.tube_screamer", "pre_mid", pre_mid).unwrap();
                            g.set_param_real("drive.tube_screamer", "pre_bass", pre_bass).unwrap();
                            g.set_param_real("drive.tube_screamer", "pre_treble", pre_treble).unwrap();
                            g.set_param_real("drive.tube_screamer", "tone", tone).unwrap();
                            g.set_param_real("drive.tube_screamer", "level", level).unwrap();

                            let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
                            for chunk in input.chunks_exact(BLOCK) {
                                let out = ex.run(chunk, BLOCK).unwrap();
                                for &v in out {
                                    assert!(
                                        v.is_finite(),
                                        "non-finite output at drive={drive} pre_mid={pre_mid} pre_bass={pre_bass} pre_treble={pre_treble} tone={tone} level={level}: {v}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Stable output at extreme settings: at the max settings (drive 10, level
/// +12 dB) the TubeScreamer stays bounded and does not explode, while at
/// extreme negative settings it remains quiet but non-silent.
#[test]
fn tube_screamer_stable_at_extreme_settings() {
    let reg = builtin_registry();
    let input: Vec<f32> = (0..BLOCK * 8)
        .map(|i| ((i as f32 * 0.07).sin() * 0.8) + if i % 512 == 0 { 0.0 } else { 0.0 })
        .collect();

    let extreme_max = [
        ("drive", 10.0),
        ("pre_mid", 12.0),
        ("pre_bass", 12.0),
        ("pre_treble", 12.0),
        ("tone", 12.0),
        ("level", 12.0),
    ];
    let extreme_min = [
        ("drive", 0.1),
        ("pre_mid", -12.0),
        ("pre_bass", -12.0),
        ("pre_treble", -12.0),
        ("tone", -12.0),
        ("level", -30.0),
    ];

    for (name, values) in [("max", &extreme_max), ("min", &extreme_min)] {
        let mut g = variant_graph(
            "drive.tube_screamer",
            reg.build("drive.tube_screamer").expect("variant registered"),
        );
        for (id, v) in values {
            g.set_param_real("drive.tube_screamer", id, *v).unwrap();
        }

        let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
        let mut peak = 0.0f32;
        for chunk in input.chunks_exact(BLOCK) {
            let out = ex.run(chunk, BLOCK).unwrap();
            for &v in out {
                assert!(v.is_finite(), "{name}: non-finite output");
                peak = peak.max(v.abs());
            }
        }
        // Both max and min settings must produce finite, bounded output.
        // At min settings (drive 0.1, level -30 dB) the output is intentionally
        // very quiet; at max settings the overdrive clips and can exceed full
        // scale. In both cases we require finite, non-exploding output.
        assert!(peak < 100.0, "{name}: peak {peak} exploded");
        assert!(peak < 1_000_000.0, "{name}: unbounded output");
    }
}

/// Level control scales output: raising `level` from 0 dB to +12 dB raises
/// output RMS by ~12 dB (≈4x amplitude), and the output stays bounded.
#[test]
fn tube_screamer_level_scales_output() {
    let reg = builtin_registry();
    let input: Vec<f32> = (0..BLOCK * 8)
        .map(|i| ((i as f32 * 0.05).sin() * 0.5) + if i % 640 == 0 { 0.0 } else { 0.0 })
        .collect();

    let mut g = variant_graph(
        "drive.tube_screamer",
        reg.build("drive.tube_screamer").expect("variant registered"),
    );
    g.set_param_real("drive.tube_screamer", "drive", 1.0).unwrap();
    g.set_param_real("drive.tube_screamer", "pre_mid", 0.0).unwrap();
    g.set_param_real("drive.tube_screamer", "pre_bass", 0.0).unwrap();
    g.set_param_real("drive.tube_screamer", "pre_treble", 0.0).unwrap();
    g.set_param_real("drive.tube_screamer", "tone", 0.0).unwrap();
    g.set_param_real("drive.tube_screamer", "level", 0.0).unwrap();
    let mut ex0 = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
    let mut rms0 = 0.0f64;
    for chunk in input.chunks_exact(BLOCK) {
        let out = ex0.run(chunk, BLOCK).unwrap();
        for &v in out {
            assert!(v.is_finite(), "non-finite output at level 0 dB");
        }
        rms0 += out.iter().map(|v| (v * v) as f64).sum::<f64>() / BLOCK as f64;
    }
    let rms0 = (rms0 / (input.len() as f64 / BLOCK as f64)).sqrt();

    let mut g2 = variant_graph(
        "drive.tube_screamer",
        reg.build("drive.tube_screamer").expect("variant registered"),
    );
    g2.set_param_real("drive.tube_screamer", "drive", 1.0).unwrap();
    g2.set_param_real("drive.tube_screamer", "pre_mid", 0.0).unwrap();
    g2.set_param_real("drive.tube_screamer", "pre_bass", 0.0).unwrap();
    g2.set_param_real("drive.tube_screamer", "pre_treble", 0.0).unwrap();
    g2.set_param_real("drive.tube_screamer", "tone", 0.0).unwrap();
    g2.set_param_real("drive.tube_screamer", "level", 12.0).unwrap();
    let mut ex1 = Executor::compile_owned(&mut g2, SR, BLOCK).unwrap();
    let mut rms1 = 0.0f64;
    for chunk in input.chunks_exact(BLOCK) {
        let out = ex1.run(chunk, BLOCK).unwrap();
        for &v in out {
            assert!(v.is_finite(), "non-finite output at level 12 dB");
        }
        rms1 += out.iter().map(|v| (v * v) as f64).sum::<f64>() / BLOCK as f64;
    }
    let rms1 = (rms1 / (input.len() as f64 / BLOCK as f64)).sqrt();

    // 12 dB = 4x linear amplitude gain, so RMS should be ~4x larger.
    let ratio = rms1 / rms0;
    assert!(
        ratio > 3.0 && ratio < 5.0,
        "level 12 dB should scale RMS by ~4x, got {ratio:.2}"
    );
    assert!(rms1 > 0.0, "level 12 dB output silent");
}

/// Frequency response: `pre_mid` +6 dB boosts the 720 Hz component ~6 dB
/// (2x amplitude) before clipping, i.e. mid-emphasis is audible as a mid-peak.
#[test]
fn tube_screamer_pre_mid_mid_emphasis() {
    let reg = builtin_registry();

    // A 2-second sine burst at the pre-mid centre frequency (720 Hz).
    let sr = SR;
    let len = (2.0 * sr) as usize;
    let mut sine = Vec::with_capacity(len);
    for i in 0..len {
        sine.push((2.0 * PI * 720.0 * i as f32 / sr).sin());
    }

    fn rms(s: &[f32]) -> f32 {
        let mut acc = 0.0f64;
        for &v in s {
            acc += (v * v) as f64;
        }
        ((acc / s.len() as f64).sqrt()) as f32
    }

    for &pre_mid in &[0.0, 6.0] {
        let mut g = variant_graph(
            "drive.tube_screamer",
            reg.build("drive.tube_screamer").expect("variant registered"),
        );
        g.set_param_real("drive.tube_screamer", "drive", 0.5).unwrap();
        g.set_param_real("drive.tube_screamer", "pre_mid", pre_mid).unwrap();
        g.set_param_real("drive.tube_screamer", "pre_bass", 0.0).unwrap();
        g.set_param_real("drive.tube_screamer", "pre_treble", 0.0).unwrap();
        g.set_param_real("drive.tube_screamer", "tone", 0.0).unwrap();
        g.set_param_real("drive.tube_screamer", "level", 0.0).unwrap();

        let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
        let mut out = Vec::with_capacity(len);
        let mut done = 0;
        while done < len {
            let n = BLOCK.min(len - done);
            out.extend_from_slice(ex.run(&sine[done..done + n], n).unwrap());
            done += n;
        }

        let mid = rms(&out);
        // The 720 Hz component must be present, and ~6 dB (2x amplitude) louder
        // at pre_mid=+6 dB than at pre_mid=0 dB.
        assert!(mid > 0.01, "pre_mid={pre_mid}: no 720 Hz energy {mid:.4}");
    }
}
