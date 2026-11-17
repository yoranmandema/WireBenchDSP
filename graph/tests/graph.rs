//! Graph and executor tests.
//!
//! These lock in the behaviours the architecture depends on: parameter
//! denormalisation, topological ordering, buffer disjointness, and parallel
//! recombination.

use dsp_graph::blocks::{Compressor, Gain, IoInput, IoOutput, MixSum};
use dsp_graph::{Block, BlockCtx, BlockError, Category, Executor, Graph, PortSpec, Unit};

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

/// A gain graph: in -> gain -> out.
fn gain_graph(gain_db: f32) -> Executor {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("amp", Box::new(Gain::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "amp", "in").unwrap();
    g.connect("amp", "out", "out", "in").unwrap();
    g.set_param_real("amp", "gain", gain_db).unwrap();
    Executor::compile_owned(&mut g, SR, BLOCK).unwrap()
}

/// A parallel graph: in -> (gain_a, gain_b) -> mix -> out.
fn parallel_graph(a: f32, b: f32) -> Executor {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("ga", Box::new(Gain::default())).unwrap();
    g.add("gb", Box::new(Gain::default())).unwrap();
    g.add("mix", Box::new(MixSum::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "ga", "in").unwrap();
    g.connect("in", "out", "gb", "in").unwrap();
    g.connect("ga", "out", "mix", "in_a").unwrap();
    g.connect("gb", "out", "mix", "in_b").unwrap();
    g.connect("mix", "out", "out", "in").unwrap();
    g.set_param_real("ga", "gain", a).unwrap();
    g.set_param_real("gb", "gain", b).unwrap();
    Executor::compile_owned(&mut g, SR, BLOCK).unwrap()
}

fn impulse(n: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; n];
    v[0] = 1.0;
    v
}

/// The graph stores normalised params but blocks must see real values.
/// This is the regression test for the bug that made the parallel rig silent.
#[test]
fn params_are_denormalised_before_reaching_the_block() {
    let mut ex = gain_graph(6.0);
    // +6 dB is a factor of ~2.
    let input = impulse(BLOCK);
    let out = ex.run(&input, BLOCK).unwrap();
    // The impulse is at sample 0; a gain block is memoryless, so out[0] = 2.0.
    assert!(
        (out[0] - 2.0).abs() < 0.01,
        "expected ~2.0 for +6 dB, got {}",
        out[0]
    );
}

/// A negative dB value must also survive the round trip.
#[test]
fn negative_params_are_denormalised() {
    let mut ex = gain_graph(-20.0);
    let input = impulse(BLOCK);
    let out = ex.run(&input, BLOCK).unwrap();
    let expected = pedal_dsp::util::db_to_lin(-20.0);
    assert!(
        (out[0] - expected).abs() < 1e-4,
        "expected {expected} for -20 dB, got {}",
        out[0]
    );
}

/// Two parallel branches must recombine as the weighted sum of their outputs.
/// Note the `gain` param is in **dB**, not a ratio: 0.5 means +0.5 dB.
#[test]
fn parallel_branches_recombine() {
    let mut ex = parallel_graph(0.5, -6.0);
    let input = impulse(BLOCK);
    let out = ex.run(&input, BLOCK).unwrap();

    // Branch A: +0.5 dB -> 1.0593; branch B: -6 dB -> 0.5012.
    let expected = pedal_dsp::util::db_to_lin(0.5) + pedal_dsp::util::db_to_lin(-6.0);
    assert!(
        (out[0] - expected).abs() < 1e-4,
        "expected {expected}, got {}",
        out[0]
    );
}

/// The output of a parallel graph must not be silent.
#[test]
fn parallel_output_is_not_silent() {
    let mut ex = parallel_graph(0.0, 0.0);
    let mut input = vec![0.0f32; BLOCK * 4];
    for (i, s) in input.iter_mut().enumerate() {
        *s = (i as f32 * 0.1).sin();
    }
    let mut peak = 0.0f32;
    for b in 0..4 {
        let out = ex.run(&input[b * BLOCK..(b + 1) * BLOCK], BLOCK).unwrap();
        for &v in out {
            peak = peak.max(v.abs());
        }
    }
    assert!(peak > 0.1, "parallel output is silent (peak {peak})");
}

/// The step list must be in topological order: a node never runs before its
/// sources.
#[test]
fn steps_are_in_topological_order() {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("a", Box::new(Gain::default())).unwrap();
    g.add("b", Box::new(Gain::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "a", "in").unwrap();
    g.connect("a", "out", "b", "in").unwrap();
    g.connect("b", "out", "out", "in").unwrap();
    let ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();

    let names: Vec<&str> = ex.steps().iter().map(|s| s.node.as_str()).collect();
    let pos = |n: &str| names.iter().position(|&x| x == n).unwrap();
    assert!(pos("in") < pos("a"), "in must precede a: {names:?}");
    assert!(pos("a") < pos("b"), "a must precede b: {names:?}");
    assert!(pos("b") < pos("out"), "b must precede out: {names:?}");
}

/// No step may use the same buffer slot for an input and an output — that would
/// be an aliasing view.
#[test]
fn buffer_slots_are_disjoint_within_a_step() {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("a", Box::new(Gain::default())).unwrap();
    g.add("b", Box::new(Gain::default())).unwrap();
    g.add("mix", Box::new(MixSum::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "a", "in").unwrap();
    g.connect("in", "out", "b", "in").unwrap();
    g.connect("a", "out", "mix", "in_a").unwrap();
    g.connect("b", "out", "mix", "in_b").unwrap();
    g.connect("mix", "out", "out", "in").unwrap();
    let ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();

    for step in ex.steps() {
        for i in &step.inputs {
            for o in &step.outputs {
                assert_ne!(
                    i, o,
                    "step '{}' aliases input slot {:?} with output slot {:?}",
                    step.node, i, o
                );
            }
        }
    }
}

/// An unconnected audio input is a compile error, not a silent runtime bug.
#[test]
fn missing_input_is_rejected() {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    // IoOutput's input is not driven.
    let err = Executor::compile_owned(&mut g, SR, BLOCK).unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("MissingInput"), "got: {msg}");
}

/// A cycle is rejected in v1.
#[test]
fn cycle_is_rejected() {
    // in -> mix.a; mix -> a -> b -> mix.b  (a loop through the mixer)
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("mix", Box::new(MixSum::default())).unwrap();
    g.add("a", Box::new(Gain::default())).unwrap();
    g.add("b", Box::new(Gain::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "mix", "in_a").unwrap();
    g.connect("mix", "out", "a", "in").unwrap();
    g.connect("a", "out", "b", "in").unwrap();
    g.connect("b", "out", "mix", "in_b").unwrap();
    g.connect("mix", "out", "out", "in").unwrap();
    let err = Executor::compile_owned(&mut g, SR, BLOCK).unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("Cycle"), "got: {msg}");
}

/// Parameters can be changed after compilation, and the block sees the new
/// real value.
#[test]
fn param_change_after_compile_is_applied() {
    let mut ex = gain_graph(0.0);
    let input = impulse(BLOCK);
    let out0 = ex.run(&input, BLOCK).unwrap()[0];
    assert!((out0 - 1.0).abs() < 1e-4, "0 dB should be unity, got {out0}");

    ex.set_param_real("amp", "gain", 12.0).unwrap();
    let out1 = ex.run(&input, BLOCK).unwrap()[0];
    let expected = pedal_dsp::util::db_to_lin(12.0);
    assert!(
        (out1 - expected).abs() < 0.01,
        "expected {expected} after +12 dB, got {out1}"
    );
}

/// The registry builds a block from its id.
#[test]
fn registry_builds_blocks() {
    let mut r = dsp_graph::Registry::new();
    r.add::<Gain>();
    r.add::<MixSum>();
    assert!(r.get("util.gain").is_some());
    assert!(r.get("util.mix").is_some());
    assert!(r.get("nope").is_none());
    let b = r.build("util.gain").unwrap();
    assert_eq!(b.spec().id, "util.gain");
}

/// A block's spec declares the params the UI will render.
#[test]
fn gain_spec_declares_its_params() {
    let g = Gain::default();
    let spec = g.spec();
    assert_eq!(spec.id, "util.gain");
    assert_eq!(spec.params.len(), 1);
    assert_eq!(spec.params[0].id, "gain");
    assert_eq!(spec.params[0].unit, Unit::Decibels);
    assert_eq!(spec.inputs.len(), 1);
    assert_eq!(spec.outputs.len(), 1);
}

// --- latency compensation ---------------------------------------------------

/// A pure N-sample delay that *declares* its delay as algorithmic latency,
/// standing in for a FIR cab or a pitch shifter.
struct Latent {
    n: usize,
    buf: Vec<f32>,
    pos: usize,
}

impl Latent {
    fn new(n: usize) -> Self {
        Self { n, buf: vec![0.0; n.max(1)], pos: 0 }
    }
}

impl Block for Latent {
    fn spec(&self) -> dsp_graph::BlockSpec {
        dsp_graph::BlockSpec {
            id: "test.latent",
            name: "Latent",
            category: dsp_graph::Category::Utility,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![],
            controls_in: vec![],
            controls_out: vec![],
        }
    }
    fn prepare(&mut self, _sr: f32, _max: usize) -> Result<(), BlockError> {
        Ok(())
    }
    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        let input = ctx.inputs[0];
        let out = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            if self.n == 0 {
                out[i] = input[i];
                continue;
            }
            out[i] = self.buf[self.pos];
            self.buf[self.pos] = input[i];
            self.pos = (self.pos + 1) % self.n;
        }
        Ok(())
    }
    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
    }
    fn latency_samples(&self) -> u32 {
        self.n as u32
    }
}

/// in -> [latent chain] + [dry gain] -> mix -> out
fn latent_parallel(lat: &[usize]) -> Executor {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    let mut prev = "in".to_string();
    for (k, &n) in lat.iter().enumerate() {
        let id = format!("lat{k}");
        g.add(id.clone(), Box::new(Latent::new(n))).unwrap();
        g.connect(&prev, "out", &id, "in").unwrap();
        prev = id;
    }
    g.add("dry", Box::new(Gain::default())).unwrap();
    g.add("mix", Box::new(MixSum::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "dry", "in").unwrap();
    g.connect(&prev, "out", "mix", "in_a").unwrap();
    g.connect("dry", "out", "mix", "in_b").unwrap();
    g.connect("mix", "out", "out", "in").unwrap();
    Executor::compile_owned(&mut g, SR, BLOCK).unwrap()
}

/// Run an impulse through `blocks` blocks and return the concatenated output.
fn impulse_response(ex: &mut Executor, blocks: usize) -> Vec<f32> {
    let mut input = vec![0.0f32; BLOCK * blocks];
    input[0] = 1.0;
    let mut out = Vec::with_capacity(input.len());
    for b in 0..blocks {
        out.extend_from_slice(ex.run(&input[b * BLOCK..(b + 1) * BLOCK], BLOCK).unwrap());
    }
    out
}

fn assert_single_peak(ir: &[f32], at: usize, value: f32) {
    for (i, &v) in ir.iter().enumerate() {
        let want = if i == at { value } else { 0.0 };
        assert!(
            (v - want).abs() < 1e-5,
            "sample {i}: expected {want}, got {v} (peak should be only at {at})"
        );
    }
}

/// A latent branch in parallel with a dry branch: the dry branch must be
/// delayed so both impulses land on the same sample and sum to 2.0, instead
/// of two 1.0 spikes N samples apart (comb filtering).
#[test]
fn latency_pass_aligns_parallel_branches() {
    let n = 100; // > BLOCK, so the delay crosses a block boundary
    let mut ex = latent_parallel(&[n]);
    assert_eq!(ex.total_latency(), n as u32);
    assert_eq!(ex.delay_count(), 1, "exactly the dry branch needs a delay");
    let ir = impulse_response(&mut ex, 4);
    assert_single_peak(&ir, n, 2.0);
}

/// Latency accumulates through a series chain inside a branch.
#[test]
fn latency_accumulates_in_series() {
    let mut ex = latent_parallel(&[30, 45]);
    assert_eq!(ex.total_latency(), 75);
    assert_eq!(ex.node_latency("lat1"), Some(75));
    let ir = impulse_response(&mut ex, 3);
    assert_single_peak(&ir, 75, 2.0);
}

/// Zero-latency graphs must not get any compensation delays.
#[test]
fn zero_latency_graph_inserts_no_delays() {
    let ex = parallel_graph(0.0, 0.0);
    assert_eq!(ex.total_latency(), 0);
    assert_eq!(ex.delay_count(), 0);
}

/// Reset must clear compensation delay lines too, or a preset change leaks
/// the previous preset's audio.
#[test]
fn reset_clears_compensation_delays() {
    let mut ex = latent_parallel(&[100]);
    let _ = impulse_response(&mut ex, 1); // impulse is now inside the delays
    ex.reset();
    let silent = vec![0.0f32; BLOCK];
    for _ in 0..4 {
        let out = ex.run(&silent, BLOCK).unwrap();
        assert!(out.iter().all(|&v| v == 0.0), "audio leaked across reset");
    }
}

// --- editor-shaped graphs ----------------------------------------------------

/// A block whose output goes nowhere must not hijack the graph output, even
/// if it sorts after `io.output`. (The executor used to take the last step's
/// input as the output.)
#[test]
fn dangling_branch_does_not_become_the_output() {
    for _ in 0..20 {
        // HashMap order varies per graph; try several.
        let mut g = Graph::new();
        g.add("in", Box::new(IoInput)).unwrap();
        g.add("amp", Box::new(Gain::default())).unwrap();
        g.add("dangling", Box::new(Gain::default())).unwrap();
        g.add("out", Box::new(IoOutput)).unwrap();
        g.connect("in", "out", "amp", "in").unwrap();
        g.connect("in", "out", "dangling", "in").unwrap();
        g.connect("amp", "out", "out", "in").unwrap();
        g.set_param_real("amp", "gain", 6.0).unwrap();
        g.set_param_real("dangling", "gain", -40.0).unwrap();
        let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
        let out = ex.run(&impulse(BLOCK), BLOCK).unwrap();
        assert!((out[0] - 2.0).abs() < 0.01, "output came from the wrong node: {}", out[0]);
    }
}

#[test]
fn two_wires_into_one_input_is_rejected() {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("a", Box::new(Gain::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "a", "in").unwrap();
    g.connect("in", "out", "out", "in").unwrap();
    g.connect("a", "out", "out", "in").unwrap();
    let msg = format!("{}", Executor::compile_owned(&mut g, SR, BLOCK).unwrap_err());
    assert!(msg.contains("more than one wire"), "got: {msg}");
}

#[test]
fn graph_without_output_is_rejected() {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("a", Box::new(Gain::default())).unwrap();
    g.connect("in", "out", "a", "in").unwrap();
    let msg = format!("{}", Executor::compile_owned(&mut g, SR, BLOCK).unwrap_err());
    assert!(msg.contains("exactly one output"), "got: {msg}");
}

/// The real-time parameter path: by index, clamped into range.
#[test]
fn set_param_at_by_index_and_clamped() {
    let mut ex = gain_graph(0.0);
    let amp = ex.node_ids().iter().position(|id| id == "amp").unwrap();
    assert!(ex.set_param_at(amp, 0, 6.0));
    let out = ex.run(&impulse(BLOCK), BLOCK).unwrap();
    assert!((out[0] - 2.0).abs() < 0.01);
    assert!(ex.set_param_at(amp, 0, 1000.0)); // clamps to +24 dB
    assert_eq!(ex.param_real("amp", "gain"), Some(24.0));
    assert!(!ex.set_param_at(amp, 7, 0.0));
    assert!(!ex.set_param_at(99, 0, 0.0));
}

#[test]
fn bypass_passes_input_through() {
    let mut ex = gain_graph(-20.0);
    let amp = ex.node_ids().iter().position(|id| id == "amp").unwrap();
    ex.set_bypass_at(amp, true);
    let out = ex.run(&impulse(BLOCK), BLOCK).unwrap();
    assert_eq!(out[0], 1.0);
    ex.set_bypass_at(amp, false);
    let out = ex.run(&impulse(BLOCK), BLOCK).unwrap();
    assert!((out[0] - 0.1).abs() < 1e-3);
}

/// The compressor is registered and builds a block whose spec declares the
/// parameters the UI renders.
#[test]
fn compressor_spec_declares_its_params() {
    let c = Compressor::default();
    let spec = c.spec();
    assert_eq!(spec.id, "dynamics.compressor");
    assert_eq!(spec.category, Category::Dynamics);
    assert_eq!(spec.params.len(), 6);
    assert_eq!(spec.params[0].id, "threshold");
    assert_eq!(spec.params[0].unit, Unit::Decibels);
    assert_eq!(spec.params[1].id, "ratio");
    assert_eq!(spec.params[1].unit, Unit::Ratio);
    assert_eq!(spec.params[2].id, "attack");
    assert_eq!(spec.params[2].unit, Unit::Milliseconds);
    assert_eq!(spec.params[3].id, "release");
    assert_eq!(spec.params[3].unit, Unit::Milliseconds);
    assert_eq!(spec.params[4].id, "makeup");
    assert_eq!(spec.params[4].unit, Unit::Decibels);
    assert_eq!(spec.params[5].id, "mix");
    assert_eq!(spec.params[5].unit, Unit::Percent);
}

/// A ratio of 1 means no gain reduction, so the signal passes through.
#[test]
fn compressor_ratio_one_passes_input_through() {
    let mut ex = compressor_graph(-20.0, 1.0);
    let comp = ex.node_ids().iter().position(|id| id == "comp").unwrap();
    ex.set_bypass_at(comp, false);
    let input = impulse(BLOCK);
    let out = ex.run(&input, BLOCK).unwrap();
    assert!((out[0] - 1.0).abs() < 1e-4, "expected ~1.0, got {}", out[0]);
}

/// A compressor reduces the level of a sustained loud signal.
///
/// At -20 dB threshold and 4:1 ratio, a -12 dBFS-ish sine settles to ~6 dB
/// of gain reduction, so the output peak lands near 0.125.
fn compressor_graph(threshold: f32, ratio: f32) -> Executor {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("comp", Box::new(Compressor::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "comp", "in").unwrap();
    g.connect("comp", "out", "out", "in").unwrap();
    g.set_param_real("comp", "threshold", threshold).unwrap();
    g.set_param_real("comp", "ratio", ratio).unwrap();
    Executor::compile_owned(&mut g, SR, BLOCK).unwrap()
}

#[test]
fn compressor_reduces_loud_sustained_signal() {
    let block = 512;
    let mut input = Vec::with_capacity(4 * block);
    for i in 0..(4 * block) {
        input.push(0.25 * (i as f32 * 0.05).sin()); // -12 dBFS-ish
    }
    let mut ex = compressor_graph(-20.0, 4.0);

    // Warm up the envelope follower so the detector is at steady state.
    for _ in 0..4 {
        let _ = ex.run(&input[..block], block).unwrap();
    }
    let mut peak = 0.0f32;
    for _ in 0..4 {
        let out = ex.run(&input[..block], block).unwrap();
        for &v in out {
            peak = peak.max(v.abs());
        }
    }
    // 4:1 ratio on a ~-12 dB signal: ~6 dB gain reduction -> 0.125.
    assert!((peak - 0.125).abs() < 0.02, "expected ~0.125, got {peak}");
}

/// Below the threshold the compressor must not touch the signal.
#[test]
fn compressor_below_threshold_passes_input_through() {
    let mut ex = compressor_graph(-20.0, 20.0);
    let comp = ex.node_ids().iter().position(|id| id == "comp").unwrap();
    ex.set_bypass_at(comp, false);
    let input = impulse(BLOCK);
    let out = ex.run(&input, BLOCK).unwrap();
    assert!((out[0] - 1.0).abs() < 1e-4, "expected ~1.0, got {}", out[0]);
}
