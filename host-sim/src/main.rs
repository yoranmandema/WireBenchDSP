//! Host simulator: render a graph over a WAV file, and measure it.
//!
//! Runs the *same* executor code that will run on the Pi.
//!
//! ```bash
//! cargo run --release -p host-sim -- --in input.wav --out output.wav
//! cargo run --release -p host-sim -- --in input.wav --bench
//! cargo run --release -p host-sim -- --in input.wav --impulse
//! ```

use std::path::PathBuf;
use std::time::Instant;

use dsp_graph::{Executor, Graph, Registry};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

mod blocks;
use blocks::*;

/// The default registry — the palette.
fn registry() -> Registry {
    let mut r = Registry::new();
    r.add::<Gain>();
    r.add::<ToneStack>();
    r.add::<Overdrive>();
    r.add::<Reverb>();
    r.add::<Delay>();
    r
}

/// Print the palette: every registered block and its params.
fn print_palette() {
    let r = registry();
    println!("block palette:");
    let mut specs: Vec<_> = r.specs().collect();
    specs.sort_by_key(|s| s.id);
    for s in specs {
        println!("  {:24} {}", s.id, s.name);
        for p in &s.params {
            println!("      {:14} {:>8} .. {:<8} {}", p.name, p.min, p.max, p.unit);
        }
    }
}

/// A hand-built demo rig: drive -> tone -> delay -> reverb -> out.
fn demo_graph() -> Graph {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("drive", Box::new(Overdrive::default())).unwrap();
    g.add("tone", Box::new(ToneStack::default())).unwrap();
    g.add("delay", Box::new(Delay::default())).unwrap();
    g.add("verb", Box::new(Reverb::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();

    g.connect("in", "out", "drive", "in").unwrap();
    g.connect("drive", "out", "tone", "in").unwrap();
    g.connect("tone", "out", "delay", "in").unwrap();
    g.connect("delay", "out", "verb", "in").unwrap();
    g.connect("verb", "out", "out", "in").unwrap();

    g.set_param_real("drive", "drive", 4.0).unwrap();
    g.set_param_real("drive", "level", -3.0).unwrap();
    g.set_param_real("tone", "low", 2.0).unwrap();
    g.set_param_real("tone", "high", -3.0).unwrap();
    g.set_param_real("delay", "time", 280.0).unwrap();
    g.set_param_real("delay", "mix", 25.0).unwrap();
    g.set_param_real("verb", "mix", 25.0).unwrap();
    g
}

/// A parallel bass rig: clean low + driven high, recombined.
fn parallel_bass_graph() -> Graph {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("clean", Box::new(Gain::default())).unwrap();
    g.add("drive", Box::new(Overdrive::default())).unwrap();
    g.add("tone", Box::new(ToneStack::default())).unwrap();
    g.add("mix", Box::new(MixSum::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();

    // Two parallel branches: clean path and driven path, both into Mix.
    g.connect("in", "out", "clean", "in").unwrap();
    g.connect("in", "out", "drive", "in").unwrap();
    g.connect("drive", "out", "tone", "in").unwrap();
    g.connect("clean", "out", "mix", "in_a").unwrap();
    g.connect("tone", "out", "mix", "in_b").unwrap();
    g.connect("mix", "out", "out", "in").unwrap();

    g.set_param_real("clean", "gain", 0.0).unwrap();
    g.set_param_real("drive", "drive", 6.0).unwrap();
    g.set_param_real("tone", "high", -6.0).unwrap();
    g.set_param_real("mix", "a", 70.0).unwrap();
    g.set_param_real("mix", "b", 50.0).unwrap();
    g
}

/// The graph source: either the demo rig or the parallel bass rig.
fn build_graph(which: &str) -> Graph {
    match which {
        "parallel" => parallel_bass_graph(),
        _ => demo_graph(),
    }
}

fn read_wav(path: &PathBuf) -> (Vec<f32>, u32) {
    let mut reader = WavReader::open(path).expect("could not open input WAV");
    let spec = reader.spec();
    eprintln!(
        "input : {}  ({} Hz, {} ch, {:?} {}-bit)",
        path.display(),
        spec.sample_rate,
        spec.channels,
        spec.sample_format,
        spec.bits_per_sample
    );
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|s| s.expect("bad i16") as f32 / 32768.0)
            .collect(),
        (SampleFormat::Int, 24) => reader
            .samples::<i32>()
            .map(|s| s.expect("bad i24") as f32 / 8_388_608.0)
            .collect(),
        (SampleFormat::Int, 32) => reader
            .samples::<i32>()
            .map(|s| s.expect("bad i32") as f32 / 2_147_483_648.0)
            .collect(),
        (SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .map(|s| s.expect("bad f32"))
            .collect(),
        other => panic!("unsupported sample format: {other:?}"),
    };
    (samples, spec.sample_rate)
}

fn write_wav(path: &PathBuf, samples: &[f32], sample_rate: u32) {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 24,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).expect("could not create output WAV");
    for &y in samples {
        let v = (y.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
        writer.write_sample(v).expect("write sample");
    }
    writer.finalize().expect("finalize WAV");
}

fn synth_test_signal(sample_rate: u32, seconds: f32) -> Vec<f32> {
    // A plucked-string-ish burst: a decaying harmonic stack, then silence.
    let n = (sample_rate as f32 * seconds) as usize;
    let mut out = vec![0.0f32; n];
    let f0 = 110.0; // A2
    let total = (sample_rate as f32 * 0.4) as usize; // 0.4 s of tone
    for i in 0..total.min(n) {
        let t = i as f32 / sample_rate as f32;
        let env = (-t * 9.0).exp();
        let mut s = 0.0;
        for (h, amp) in [(1, 1.0), (2, 0.5), (3, 0.25), (4, 0.12), (5, 0.06)] {
            s += amp * (2.0 * std::f32::consts::PI * f0 * h as f32 * t).sin();
        }
        out[i] = s * env * 0.5;
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut input = PathBuf::from("input.wav");
    let mut output = PathBuf::from("output.wav");
    let mut bench = false;
    let mut impulse = false;
    let mut rig = "demo".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--in" => { input = PathBuf::from(&args[i + 1]); i += 2; }
            "--out" => { output = PathBuf::from(&args[i + 1]); i += 2; }
            "--bench" => { bench = true; i += 1; }
            "--impulse" => { impulse = true; i += 1; }
            "--rig" => { rig = args[i + 1].clone(); i += 2; }
            "--palette" => { print_palette(); return; }
            other => { eprintln!("unknown arg: {other}"); std::process::exit(2); }
        }
    }

    let sample_rate = 48_000u32;
    let block_size = 64usize;

    // Build and compile.
    let mut graph = build_graph(&rig);
    let mut executor = match Executor::compile_owned(&mut graph, sample_rate as f32, block_size) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("compile failed: {e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "compiled: {} nodes, {} steps",
        executor.node_count(),
        executor.steps().len()
    );

    let samples = if impulse {
        let mut v = vec![0.0f32; (sample_rate as f32 * 0.5) as usize];
        v[0] = 1.0;
        eprintln!("input : synthetic impulse (0.5 s)");
        v
    } else if input.exists() {
        let (s, sr) = read_wav(&input);
        if sr != sample_rate {
            eprintln!("warning: input is {sr} Hz, graph is tuned for {sample_rate} Hz");
        }
        s
    } else {
        eprintln!("input: {input:?} not found — synthesising a test tone");
        synth_test_signal(sample_rate, 3.0)
    };

    // Process in blocks.
    let n_blocks = samples.len().div_ceil(block_size);
    let mut out = vec![0.0f32; samples.len()];
    let mut peak = 0.0f32;
    let mut clip = 0usize;
    let mut sum_sq = 0.0f64;
    let mut nan = 0usize;
    let mut total_cycles = 0u128;

    let start = Instant::now();
    for b in 0..n_blocks {
        let lo = b * block_size;
        let hi = ((b + 1) * block_size).min(samples.len());
        let n = hi - lo;
        let mut block = vec![0.0f32; block_size];
        block[..n].copy_from_slice(&samples[lo..hi]);

        let t0 = std::time::Instant::now();
        let result = executor.run(&block, block_size);
        let dt = t0.elapsed();
        total_cycles += dt.as_nanos();

        match result {
            Ok(y) => {
                for i in 0..n {
                    let v = y[i];
                    if !v.is_finite() {
                        nan += 1;
                        out[lo + i] = 0.0;
                    } else {
                        out[lo + i] = v;
                        if v.abs() > peak { peak = v.abs(); }
                        if v.abs() > 1.0 { clip += 1; }
                        sum_sq += (v as f64) * (v as f64);
                    }
                }
            }
            Err(e) => {
                eprintln!("block {b} failed: {e}");
                std::process::exit(1);
            }
        }
    }
    let elapsed = start.elapsed();

    // Report.
    let rms = (sum_sq / out.len().max(1) as f64).sqrt();
    let audio_seconds = samples.len() as f32 / sample_rate as f32;
    let cpu_percent = if audio_seconds > 0.0 {
        (elapsed.as_secs_f64() / audio_seconds as f64) * 100.0
    } else {
        0.0
    };

    println!();
    println!("  rig        : {rig}");
    println!("  samples    : {}", samples.len());
    println!("  block      : {block_size} frames ({} ms)", block_size as f32 / sample_rate as f32 * 1000.0);
    println!("  peak       : {peak:.4}");
    println!("  rms        : {rms:.4}");
    println!("  clipped    : {clip}");
    println!("  non-finite : {nan}");
    println!("  render     : {:.3?} for {:.2}s audio", elapsed, audio_seconds);
    println!("  cpu        : {cpu_percent:.1}% of one core (Mac, debug build)");
    if nan > 0 {
        eprintln!("  WARNING: {nan} non-finite samples — unstable graph");
    }
    if peak > 1.0 {
        eprintln!("  note: peak exceeds full scale; the output will be clamped");
    }

    if bench {
        // Per-block cost in ns, and as a fraction of the real-time budget.
        let per_block_ns = total_cycles as f64 / n_blocks.max(1) as f64;
        let budget_ns = block_size as f64 / sample_rate as f64 * 1e9;
        println!();
        println!("  per-block  : {per_block_ns:.0} ns  (budget {budget_ns:.0} ns)");
        println!("  headroom   : {:.1}% of the block budget", per_block_ns / budget_ns * 100.0);
    }

    write_wav(&output, &out, sample_rate);
    eprintln!("output: {}", output.display());
}
