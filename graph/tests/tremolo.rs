//! Unit tests for the tremolo block.
//!
//! The Tremolo is amplitude modulation: the input gain is swept by an LFO so
//! the volume pulses rhythmically, with the gain varying between `(1 -
//! depth)` and `1`. These tests lock the behaviours the UI, the JSON loader
//! and the validator depend on.
//!
//!     cargo test -p dsp-graph --test tremolo

use dsp_graph::blocks::{IoInput, IoOutput, Tremolo};
use dsp_graph::{Category, Executor, Graph, PortSpec, Unit};

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;

/// A minimal two-block graph: `in -> Tremolo -> out`.
///
/// `max_block` controls the executor's per-run frame limit. Long runs (one
/// full LFO cycle or multi-block phase continuity) need a larger limit than
/// the default `BLOCK`.
fn tremolo_graph(rate: f32, depth: f32, mix: f32, max_block: usize) -> Executor {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("trem", Box::new(Tremolo::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "trem", "in").unwrap();
    g.connect("trem", "out", "out", "in").unwrap();
    g.set_param_real("trem", "rate", rate).unwrap();
    g.set_param_real("trem", "depth", depth).unwrap();
    g.set_param_real("trem", "mix", mix).unwrap();
    Executor::compile_owned(&mut g, SR, max_block).unwrap()
}

/// The registry must list the tremolo exactly like the other effects: same
/// shape, same categories, params the UI can render.
#[test]
fn tremolo_is_registered() {
    let mut r = dsp_graph::Registry::new();
    r.add::<Tremolo>();
    let spec = r
        .get("mod.tremolo")
        .expect("registry must list mod.tremolo");
    assert_eq!(spec.id, "mod.tremolo");
    assert_eq!(spec.name, "Tremolo");
    assert_eq!(spec.category, Category::Modulation);
    assert_eq!(spec.inputs.len(), 1);
    assert_eq!(spec.inputs[0].name, "in");
    assert_eq!(spec.inputs[0].kind, PortSpec::audio("in").kind);
    assert_eq!(spec.outputs.len(), 1);
    assert_eq!(spec.outputs[0].name, "out");
    assert_eq!(spec.outputs[0].kind, PortSpec::audio("out").kind);

    let params = &spec.params;
    assert_eq!(params.len(), 3, "expected rate, depth and mix");
    assert_eq!(params[0].id, "rate");
    assert_eq!(params[0].name, "Rate");
    assert_eq!(params[0].unit, Unit::Hertz);
    assert!((params[0].min - 0.1).abs() < 1e-6);
    assert!((params[0].max - 5.0).abs() < 1e-6);
    assert!((params[0].default - 0.6).abs() < 1e-6);
    assert_eq!(params[1].id, "depth");
    assert_eq!(params[1].name, "Depth");
    assert_eq!(params[1].unit, Unit::Linear);
    assert!((params[1].min - 0.0).abs() < 1e-6);
    assert!((params[1].max - 1.0).abs() < 1e-6);
    assert!((params[1].default - 0.5).abs() < 1e-6);
    assert_eq!(params[2].id, "mix");
    assert_eq!(params[2].name, "Mix");
    assert_eq!(params[2].unit, Unit::Percent);
    assert!((params[2].min - 0.0).abs() < 1e-6);
    assert!((params[2].max - 100.0).abs() < 1e-6);
    assert!((params[2].default - 100.0).abs() < 1e-6);
}

/// `depth = 0` must be a bypass: the output is sample-identical to the input
/// for every supported mix value (dry, wet and anything in between).
#[test]
fn tremolo_depth_zero_bypasses_input() {
    let input = vec![0.37f32; BLOCK];
    for mix in [0.0f32, 50.0, 100.0] {
        let mut ex = tremolo_graph(0.6, 0.0, mix, BLOCK);
        let out = ex.run(&input, BLOCK).unwrap();
        for (i, &v) in out.iter().enumerate() {
            assert!(
                (v - input[i]).abs() < 1e-6,
                "mix {mix}: sample {i} expected {} input, got {v}",
                input[i]
            );
        }
    }
}

/// Out-of-range values must clamp instead of panicking.
#[test]
fn tremolo_params_allow_out_of_range_and_clamp() {
    // The executor clamps params into the declared range before handing them
    // to the block, so a rate above the maximum must land on the max.
    let above = 99.0;
    let mut ex = tremolo_graph(above, 0.0, 100.0, BLOCK);
    let trem = ex.node_ids().iter().position(|id| id == "trem").unwrap();
    assert!(ex.set_param_at(trem, 0, above));
    assert!((ex.param_real("trem", "rate").unwrap() - 5.0).abs() < 1e-6);
}

/// A constant-amplitude input must come out with an oscillating envelope at
/// the configured rate: the gain swings between `(1 - depth)` and `1`, so
/// with `depth = 0.5` (rate 5 Hz, 1 s) the output ranges between roughly
/// `[0.25, 0.5]` (a 0.5-amplitude input). A static gain or a broken LFO
/// would leave the envelope flat and fail this.
#[test]
fn tremolo_modulates_a_constant_amplitude_input() {
    // 1 s at 48 kHz gives a 1 Hz LFO exactly one full cycle, so both peak
    // and trough are exercised.
    let n = 48_000;
    let input = vec![0.5f32; n];
    let mut ex = tremolo_graph(1.0, 0.5, 100.0, n);
    let out = ex.run(&input, n).unwrap();

    let peak = out.iter().copied().fold(0.0f32, f32::max);
    let trough = out.iter().copied().fold(f32::MAX, f32::min);
    assert!(peak > 0.49, "expected a peak near 0.5, got {peak}");
    assert!(trough < 0.35, "expected the depth to reach into the trough (<0.35), got {trough}");

    // The output must stay finite and within the expected modulation band.
    for (i, &v) in out.iter().enumerate() {
        assert!(
            v <= 0.5 + 1e-6 && v >= 0.25 - 1e-6,
            "sample {i}: expected output in [0.25, 0.5], got {v}"
        );
    }
    assert!(out.iter().all(|&v| v.is_finite()), "non-finite output");
}

/// Depth and rate must interact: a faster rate must still modulate at that
/// rate, and a deeper depth must swing further.
#[test]
fn tremolo_depth_and_rate_control_the_swing() {
    let n = 48_000;
    let input = vec![0.5f32; n];

    // depth 0.5 -> band [0.25, 0.5]
    {
        let mut ex = tremolo_graph(1.0, 0.5, 100.0, n);
        let out = ex.run(&input, n).unwrap();
        let peak = out.iter().copied().fold(0.0f32, f32::max);
        let trough = out.iter().copied().fold(f32::MAX, f32::min);
        assert!(peak > 0.49, "depth 0.5 peak {peak}");
        assert!(trough < 0.35, "depth 0.5 trough {trough}");
    }

    // depth 0.25 -> band [0.375, 0.5]
    {
        let mut ex = tremolo_graph(1.0, 0.25, 100.0, n);
        let out = ex.run(&input, n).unwrap();
        let peak = out.iter().copied().fold(0.0f32, f32::max);
        let trough = out.iter().copied().fold(f32::MAX, f32::min);
        assert!(peak > 0.49, "depth 0.25 peak {peak}");
        assert!(trough > 0.36, "depth 0.25 trough {trough} (expected deeper than 0.375-ish)");
    }

    // rate 2 Hz over the same 1 s buffer completes two cycles: the output
    // must cross the 0.35-modulation threshold twice (two distinct
    // downward troughs), not once.
    {
        let mut ex = tremolo_graph(2.0, 0.5, 100.0, n);
        let out = ex.run(&input, n).unwrap();
        let mut troughs = 0usize;
        let mut prev = out[0];
        for &v in &out[1..] {
            if prev > 0.35 && v <= 0.35 {
                troughs += 1;
            }
            prev = v;
        }
        assert!(troughs >= 1, "expected at least one trough for a 2 Hz LFO over 1 s, got {troughs}");
    }
}

/// Consecutive runs of `N` samples on the SAME block must reproduce one run
/// of `2N` samples. The LFO phase advances monotonically and only ever
/// wraps, so it carries over across block boundaries — there is no reset at
/// a boundary, so no discontinuity (no click).
///
/// Each `run` is capped at `max_block` frames, so the single long run needs
/// `max_block >= 2N`; the two halves run on the same executor, so the phase
/// keeps advancing instead of resetting.
#[test]
fn tremolo_phase_is_continuous_across_blocks() {
    let n = 256;
    let big_block = n * 2; // 512
    let input = vec![0.5f32; big_block];

    // Single run of the whole block: the reference.
    let mut ex_single = tremolo_graph(1.0, 0.5, 100.0, big_block);
    let single = ex_single
        .run(&input, big_block)
        .expect("compile and run")
        .to_vec();

    // Consecutive runs on the SAME block: the LFO phase must be continuous
    // across the block boundary. Only one `run` slice is live at a time, so
    // each is cloned and compared immediately.
    let mut ex = tremolo_graph(1.0, 0.5, 100.0, big_block);
    let first = ex.run(&input[..n], n).unwrap().to_vec();
    assert_eq!(
        first.as_slice(),
        &single[..n],
        "first block boundary is discontinuous"
    );

    let second = ex.run(&input[n..], n).unwrap().to_vec();
    assert_eq!(
        second.as_slice(),
        &single[n..],
        "second block boundary is discontinuous"
    );
}

/// A reset must clear the LFO state so a fresh run starts at a known phase
/// (no leftover modulation from a previous preset).
#[test]
fn tremolo_reset_clears_lfo() {
    let n = 48_000;
    let input = vec![0.5f32; n];
    let mut ex = tremolo_graph(1.0, 0.5, 100.0, n);

    // Own the output data before calling `reset`, which reborrows `ex`.
    let out_before = ex.run(&input, n).unwrap().to_vec();

    ex.reset();

    // After reset the LFO is at phase 0 again, so the very first sample is a
    // deterministic mid-modulation point (depth 0.5 -> gain 0.75 -> 0.375).
    assert!(
        (out_before[0] - 0.375).abs() < 1e-6,
        "expected 0.375 after reset, got {}",
        out_before[0]
    );
}

/// The executor's frame cap is respected and the block always writes every
/// output frame.
#[test]
fn tremolo_writes_all_frames_without_allocation() {
    let n = 128;
    let mut ex = tremolo_graph(0.6, 0.5, 100.0, n);
    let input = vec![0.25f32; n];
    let out = ex.run(&input, n).unwrap();
    assert_eq!(out.len(), n);
    assert!(out.iter().all(|v| v.is_finite()), "output is not finite");
}
