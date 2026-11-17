//! US-5.4: the chorus block under the Modulation category.
//!
//! Wraps `pedal_dsp::delay::Chorus` (an allpass fractional delay modulated by
//! an LFO, with feedback and a dry/wet mix) via the `Chorus` block. This is
//! parameter mapping plus integration — no new DSP.

use dsp_graph::blocks::{builtin_registry, Chorus};
use dsp_graph::doc::Doc;
use dsp_graph::{Executor, Graph};
use dsp_graph::types::{Category, PortKind, Unit};

const SR: f32 = 48_000.0;
const BLOCK: usize = 4096;

/// Assert a parameter declaration in a `BlockSpec`.
fn assert_param(spec: &dsp_graph::types::BlockSpec, id: &str, name: &str, unit: Unit,
                min: f32, max: f32, default: f32, curve: dsp_graph::types::Curve) {
    let p = spec
        .params
        .iter()
        .find(|p| p.id == id)
        .expect("parameter id present");
    assert_eq!(p.name, name, "parameter {id}");
    assert_eq!(p.unit, unit, "parameter {id}");
    assert!(
        (p.min - min).abs() < 1e-6,
        "parameter {id} min: {} != {}",
        p.min,
        min
    );
    assert!(
        (p.max - max).abs() < 1e-6,
        "parameter {id} max: {} != {}",
        p.max,
        max
    );
    assert!(
        (p.default - default).abs() < 1e-6,
        "parameter {id} default: {} != {}",
        p.default,
        default
    );
    assert_eq!(p.curve, curve, "parameter {id}");
}

fn chorus_spec() -> dsp_graph::types::BlockSpec {
    builtin_registry()
        .get("mod.chorus")
        .expect("mod.chorus should be registered")
        .clone()
}

fn audio_chain() -> (Graph, usize) {
    let mut g = Graph::new();
    g.add("in", Box::new(dsp_graph::blocks::IoInput)).unwrap();
    g.add("chorus", Box::new(Chorus::default())).unwrap();
    g.add("out", Box::new(dsp_graph::blocks::IoOutput)).unwrap();
    g.connect("in", "out", "chorus", "in").unwrap();
    g.connect("chorus", "out", "out", "in").unwrap();
    (g, BLOCK)
}

/// Normalized cross-correlation of `input` against `output` over lags
/// `0..=max_lag`. Returns the lag with the highest correlation.
fn estimate_lag(input: &[f32], output: &[f32], max_lag: usize) -> (usize, f32) {
    let n = input.len();
    assert_eq!(output.len(), n);
    let mut best_lag = 0;
    let mut best_corr = -1.0f32;
    for lag in 0..=max_lag {
        let m = n - lag;
        if m < 64 {
            continue;
        }
        let mut num = 0.0f32;
        let mut sum_in = 0.0f32;
        let mut sum_out = 0.0f32;
        let mut sum_in2 = 0.0f32;
        let mut sum_out2 = 0.0f32;
        for i in 0..m {
            let j = i + lag;
            let xi = input[i];
            let oj = output[j];
            num += xi * oj;
            sum_in += xi;
            sum_out += oj;
            sum_in2 += xi * xi;
            sum_out2 += oj * oj;
        }
        let mean_in = sum_in / m as f32;
        let mean_out = sum_out / m as f32;
        let cov = num / m as f32 - mean_in * mean_out;
        let var_in = sum_in2 / m as f32 - mean_in * mean_in;
        let var_out = sum_out2 / m as f32 - mean_out * mean_out;
        if var_in < 1e-9 || var_out < 1e-9 {
            continue;
        }
        let denom = var_in.sqrt() * var_out.sqrt();
        let corr = cov / denom;
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag;
        }
    }
    (best_lag, best_corr)
}

/// Render a sine through the chorus, estimate the input→output lag per
/// window, and assert that the estimated lag sweeps (max − min ≥ 100 samples
/// at 48 kHz, i.e. ~2 ms) as the LFO sweeps the delay by ±240 samples.
#[test]
fn the_lfo_moves_the_delay() {
    let rate = 0.6f32;
    let depth = 5.0f32;
    let feedback = 0.0f32;
    let mix = 100.0f32;

    let total = (2.0 * SR) as usize;
    let window = 4096;

    let (mut g, block) = audio_chain();
    g.set_param_real("chorus", "rate", rate).unwrap();
    g.set_param_real("chorus", "depth", depth).unwrap();
    g.set_param_real("chorus", "feedback", feedback).unwrap();
    g.set_param_real("chorus", "mix", mix).unwrap();
    let mut ex = Executor::compile_owned(&mut g, SR, block).unwrap();

    let mut lags: Vec<usize> = Vec::new();
    let mut all_output = Vec::with_capacity(total);
    let mut done = 0;
    while done + window <= total {
        let start = done;
        let n = window;
        // 60 Hz: period 800 samples — longer than the lag scan (0..=700), so
        // the correlation has a single peak. A 440 Hz sine (period ~109)
        // aliases every period and the argmax hops ±109 samples regardless of
        // the LFO.
        let input: Vec<f32> = (0..n)
            .map(|i| {
                let t = (start + i) as f32 / SR;
                (2.0 * std::f32::consts::PI * 60.0 * t).sin()
            })
            .collect();
        let output = ex.run(&input, n).unwrap();
        all_output.extend_from_slice(output);

        // The block starts with an empty delay line, so its first ~13 ms of
        // output read unwritten zero slots. Run every window (so the line
        // fills as the render proceeds) but skip the first one when
        // estimating — the design's "skip the first ~20 ms while the delay
        // line fills".
        if start >= window {
            let (lag, corr) = estimate_lag(&input, output, 700);
            assert!(
                corr > 0.7,
                "window at offset {start}: correlation {corr:.2} at lag {lag}",
            );
            lags.push(lag);
        }
        done += window;
    }
    assert!(
        !lags.is_empty(),
        "no complete windows in the render",
    );
    let (min_lag, max_lag) = lags.iter().fold((lags[0], lags[0]), |(a, b), &c| {
        (a.min(c), b.max(c))
    });
    assert!(
        max_lag - min_lag >= 100,
        "lag sweep too small: lags {lags:?}",
    );

    assert!(
        all_output.iter().all(|x| x.is_finite()),
        "output contains non-finite samples",
    );
    let peak = all_output.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    assert!(
        peak < 2.0,
        "output peak {peak:.4} exceeds 2.0",
    );
}

/// `mix = 0` returns the input unchanged, so the wet/dry mix is wired the
/// right way round.
#[test]
fn dry_mix_passes_audio_through_untouched() {
    let (mut g, block) = audio_chain();
    g.set_param_real("chorus", "rate", 1.0).unwrap();
    g.set_param_real("chorus", "depth", 2.0).unwrap();
    g.set_param_real("chorus", "feedback", 0.0).unwrap();
    g.set_param_real("chorus", "mix", 0.0).unwrap();
    let mut ex = Executor::compile_owned(&mut g, SR, block).unwrap();

    let n = 2048;
    let input: Vec<f32> = (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin())
        .collect();
    let output = ex.run(&input, n).unwrap();
    assert_eq!(input, output, "mix=0 must return the input unchanged");
}

/// `mod.chorus` is present in the registry with the expected name, category,
/// ports, and parameter declarations.
#[test]
fn chorus_is_in_the_registry() {
    let spec = chorus_spec();

    assert_eq!(spec.name, "Chorus", "block name");
    assert_eq!(
        spec.category, Category::Modulation,
        "block category"
    );
    assert_eq!(spec.inputs.len(), 1, "one audio input");
    assert_eq!(spec.inputs[0].name, "in", "audio input name");
    assert_eq!(spec.inputs[0].kind, PortKind::Audio, "audio input kind");
    assert_eq!(spec.outputs.len(), 1, "one audio output");
    assert_eq!(spec.outputs[0].name, "out", "audio output name");
    assert_eq!(spec.outputs[0].kind, PortKind::Audio, "audio output kind");

    assert_param(&spec, "rate", "Rate", Unit::Hertz, 0.1, 5.0, 0.6, dsp_graph::types::Curve::Log);
    assert_param(&spec, "depth", "Depth", Unit::Milliseconds, 0.5, 5.0, 3.0, dsp_graph::types::Curve::Linear);
    assert_param(&spec, "mix", "Mix", Unit::Percent, 0.0, 100.0, 50.0, dsp_graph::types::Curve::Linear);
    assert_param(&spec, "feedback", "Feedback", Unit::Percent, 0.0, 80.0, 0.0, dsp_graph::types::Curve::Linear);
}

/// A `Doc` carrying the four parameters round-trips through JSON and renders
/// finite with bounded peak.
#[test]
fn chorus_params_round_trip_and_render() {
    let json = r#"{
        "name": "Chorus round trip",
        "nodes": [
            {
                "id": "c1",
                "block": "mod.chorus",
                "params": {
                    "rate": 1.2,
                    "depth": 1.5,
                    "mix": 75.0,
                    "feedback": 20.0
                }
            }
        ],
        "edges": []
    }"#;

    let doc = Doc::from_json(json).expect("valid round-trip JSON");
    let re_encoded = Doc::from_json(&doc.to_json()).expect("Doc is serialisable");
    assert_eq!(doc, re_encoded, "Doc round-trip");

    let mut g = Graph::new();
    g.add("in", Box::new(dsp_graph::blocks::IoInput)).unwrap();
    g.add("chorus", Box::new(Chorus::default())).unwrap();
    g.add("out", Box::new(dsp_graph::blocks::IoOutput)).unwrap();
    g.connect("in", "out", "chorus", "in").unwrap();
    g.connect("chorus", "out", "out", "in").unwrap();
    g.set_param_real("chorus", "rate", 1.2).unwrap();
    g.set_param_real("chorus", "depth", 1.5).unwrap();
    g.set_param_real("chorus", "mix", 75.0).unwrap();
    g.set_param_real("chorus", "feedback", 20.0).unwrap();
    let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();

    let total = (1.0 * SR) as usize;
    let mut out = Vec::with_capacity(total);
    let mut done = 0;
    while done < total {
        let n = BLOCK.min(total - done);
        let input: Vec<f32> = (0..n)
            .map(|i| {
                let t = (done + i) as f32 / SR;
                (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.8 * (1.0 - t).max(0.0)
            })
            .collect();
        out.extend_from_slice(ex.run(&input, n).unwrap());
        done += n;
    }

    assert!(
        out.iter().all(|x| x.is_finite()),
        "output contains non-finite samples",
    );
    let peak = out.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    assert!(
        peak <= 1.0,
        "output peak {peak:.4} exceeds 1.0",
    );
}
