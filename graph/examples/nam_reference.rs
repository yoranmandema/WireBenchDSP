//! Dump the NAM block's input and output for the tiny reference model, so a
//! Python reference render can be compared sample by sample.
//!
//! Usage: cargo run -p dsp-graph --example nam_reference -- <outdir>
//!
//! Writes `<outdir>/nam_input.raw` and `<outdir>/nam_output.raw` (little-endian
//! `f32`). The model and signal are the same ones `graph/tests/nam.rs` uses
//! (`ir_model()` and `nam_block_runs_and_passes_signal`); the companion oracle
//! is `graph/tests/nam_reference.py`.

use std::io::Write;

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

/// The tiny WaveNet model, copied from `graph/tests/nam.rs::ir_model()`.
/// One array, one layer, one channel, ReLU — enough to prove the forward pass
/// without shipping a real capture.
fn ir_model() -> String {
    serde_json::json!({
        "version": "0.5.4",
        "architecture": "WaveNet",
        "config": {
            "layers": [{
                "input_size": 1, "condition_size": 1, "channels": 1, "head_size": 1,
                "kernel_size": 1, "dilations": [1], "activation": "ReLU",
                "gated": false, "head_bias": false
            }],
            "head": null, "head_scale": 10.0
        },
        "weights": [1.0, 2.0, 0.5, 1.0, 3.0, 0.1, 0.5, 10.0],
        "sample_rate": 48000.0,
        "metadata": {"name": "Test WaveNet", "gear_type": "amp"},
    })
    .to_string()
}

fn write_raw(path: &str, samples: &[f32]) {
    let mut f = std::fs::File::create(path).unwrap();
    for &s in samples {
        f.write_all(&s.to_le_bytes()).unwrap();
    }
}

fn main() {
    let outdir = std::env::args()
        .nth(1)
        .expect("usage: nam_reference <outdir>");
    std::fs::create_dir_all(&outdir).unwrap();

    let mut g = dsp_graph::Graph::new();
    g.add("in", Box::new(dsp_graph::blocks::IoInput)).unwrap();
    let nam = dsp_graph::blocks::Nam::default();
    nam.load_json(&ir_model()).unwrap();
    assert!(nam.is_loaded());
    g.add("nam", Box::new(nam)).unwrap();
    g.add("out", Box::new(dsp_graph::blocks::IoOutput)).unwrap();
    g.connect("in", "out", "nam", "in").unwrap();
    g.connect("nam", "out", "out", "in").unwrap();
    let mut ex = dsp_graph::Executor::compile_owned(&mut g, SR, BLOCK).unwrap();

    // Same signal as `nam_block_runs_and_passes_signal`: a sine that crosses the
    // ReLU cutoff (3x + 0.5 changes sign near x = -1/6), so both branches run.
    let n = BLOCK * 8;
    let mut input = vec![0.0f32; n];
    for (i, s) in input.iter_mut().enumerate() {
        *s = (i as f32 * 0.05).sin() * 0.5;
    }
    let mut output = vec![0.0f32; n];
    for (ci, chunk) in input.chunks_exact(BLOCK).enumerate() {
        let o = ex.run(chunk, BLOCK).unwrap();
        output[ci * BLOCK..ci * BLOCK + BLOCK].copy_from_slice(o);
    }

    write_raw(&format!("{outdir}/nam_input.raw"), &input);
    write_raw(&format!("{outdir}/nam_output.raw"), &output);
    println!("wrote {n} samples to {outdir}/nam_input.raw and {outdir}/nam_output.raw");
}
