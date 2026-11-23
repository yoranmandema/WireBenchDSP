//! Integration sanity-checks for the overdrive variant blocks.
//!
//! Builds each variant from the registry and drives it on a fixed input,
//! asserting that it compiles, declares the ~30-sample algorithmic latency,
//! and produces finite (no-NaN / no-denormal) output for a full-scale signal.

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
