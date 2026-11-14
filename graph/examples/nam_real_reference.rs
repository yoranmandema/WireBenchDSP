//! Dump a real `.nam`'s input and output for the Python reference comparison.
//!
//! Usage: cargo run -p dsp-graph --example nam_real_reference -- <model.nam> <outdir> [samples]
//!
//! Writes `<outdir>/nam_input.raw` and `<outdir>/nam_output.raw` (little-endian
//! `f32`). The companion oracle is `graph/tests/nam_reference.py --model
//! <model.nam> <outdir>`; it parses the `.nam` itself and reimplements the
//! WaveNet forward pass, so the Rust block and the Python reference are
//! independent implementations of the same weights.
//!
//! The block runs at the model's own sample rate (48 kHz) with the default
//! params (0 dB in, 0 dB out, 100% mix), so the resampler is a pass-through
//! and `output == model(input)` exactly.

use std::io::Write;

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

fn write_raw(path: &str, samples: &[f32]) {
    let mut f = std::fs::File::create(path).unwrap();
    for &s in samples {
        f.write_all(&s.to_le_bytes()).unwrap();
    }
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: nam_real_reference <model.nam> <outdir> [samples]");
    let outdir = std::env::args()
        .nth(2)
        .expect("usage: nam_real_reference <model.nam> <outdir> [samples]");
    let n: usize = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(8192);
    std::fs::create_dir_all(&outdir).unwrap();

    let nam = dsp_graph::blocks::Nam::default();
    nam.load_file(&path).unwrap();
    assert!(nam.is_loaded(), "model did not load");
    println!("model: {}", nam.display_name());
    println!("expected rate: {:?}", nam.expected_rate());

    let mut g = dsp_graph::Graph::new();
    g.add("in", Box::new(dsp_graph::blocks::IoInput)).unwrap();
    g.add("nam", Box::new(nam)).unwrap();
    g.add("out", Box::new(dsp_graph::blocks::IoOutput)).unwrap();
    g.connect("in", "out", "nam", "in").unwrap();
    g.connect("nam", "out", "out", "in").unwrap();
    let mut ex = dsp_graph::Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
    assert_eq!(ex.total_latency(), 0, "no resampler delay at the model's own rate");

    // Deterministic, non-trivial signal: three tones under a slow tremolo, so
    // the nonlinearity and the recurrence are both exercised. The Python side
    // reads this file, so the exact formula only has to be reproducible once.
    let mut input = vec![0.0f32; n];
    for (i, s) in input.iter_mut().enumerate() {
        let t = i as f32 / SR;
        let env = 0.6 + 0.4 * (2.0 * std::f32::consts::PI * 0.5 * t).sin();
        let x = 0.4 * (2.0 * std::f32::consts::PI * 196.0 * t).sin()
            + 0.3 * (2.0 * std::f32::consts::PI * 392.0 * t).sin()
            + 0.2 * (2.0 * std::f32::consts::PI * 55.0 * t).sin();
        *s = x * env;
    }

    let mut output = vec![0.0f32; n];
    for (ci, chunk) in input.chunks_exact(BLOCK).enumerate() {
        let o = ex.run(chunk, BLOCK).unwrap();
        output[ci * BLOCK..ci * BLOCK + BLOCK].copy_from_slice(o);
    }

    write_raw(&format!("{outdir}/nam_input.raw"), &input);
    write_raw(&format!("{outdir}/nam_output.raw"), &output);

    let peak = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    let nonfinite = output.iter().filter(|v| !v.is_finite()).count();
    println!("wrote {n} samples to {outdir}/nam_input.raw and {outdir}/nam_output.raw");
    println!("output peak {peak:.6}  non-finite {nonfinite}");
    if nonfinite > 0 || peak <= 0.0 {
        std::process::exit(1);
    }
}
