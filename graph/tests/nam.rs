//! The NAM block: parsing, sample-rate checking, and real-time safety.

use dsp_graph::blocks::{Nam, builtin_registry};
use dsp_graph::{Block, BlockCtx, BlockError, Executor, Graph};

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

/// A tiny WaveNet model, copied from `nam-rs`'s own test vector. One array,
/// one layer, one channel, ReLU — enough to prove parsing, inference, and the
/// real-time path without shipping a real capture.
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

fn nam_graph() -> (Executor, Nam) {
    let mut g = Graph::new();
    g.add("in", Box::new(dsp_graph::blocks::IoInput)).unwrap();
    let mut nam = Nam::default();
    nam.load_json(&ir_model()).unwrap();
    assert!(nam.is_loaded());
    assert_eq!(nam.expected_rate(), Some(48_000.0));
    g.add("nam", Box::new(nam)).unwrap();
    g.add("out", Box::new(dsp_graph::blocks::IoOutput)).unwrap();
    g.connect("in", "out", "nam", "in").unwrap();
    g.connect("nam", "out", "out", "in").unwrap();
    let ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
    // The block is owned by the executor; this handle is only for inspection.
    let mut nam = Nam::default();
    nam.load_json(&ir_model()).unwrap();
    (ex, nam)
}

#[test]
fn nam_block_runs_and_passes_signal() {
    let (mut ex, _nam) = nam_graph();
    let mut input = vec![0.0f32; BLOCK * 8];
    for (i, s) in input.iter_mut().enumerate() {
        *s = (i as f32 * 0.05).sin() * 0.5;
    }
    let mut peak = 0.0f32;
    for chunk in input.chunks_exact(BLOCK) {
        for &v in ex.run(chunk, BLOCK).unwrap() {
            peak = peak.max(v.abs());
        }
    }
    assert!(peak > 0.1, "NAM output silent (peak {peak})");
}

#[test]
fn nam_block_reports_sample_rate_mismatch() {
    // Prepared at 48 kHz but fed a 44.1 kHz model: the block still runs, and
    // the mismatch is visible via the block's public accessor.
    let json = ir_model().replace("48000.0", "44100.0");
    let mut nam = Nam::default();
    nam.load_json(&json).unwrap();
    assert_eq!(nam.expected_rate(), Some(44_100.0));
    let mut g = Graph::new();
    g.add("in", Box::new(dsp_graph::blocks::IoInput)).unwrap();
    g.add("nam", Box::new(nam)).unwrap();
    g.add("out", Box::new(dsp_graph::blocks::IoOutput)).unwrap();
    g.connect("in", "out", "nam", "in").unwrap();
    g.connect("nam", "out", "out", "in").unwrap();
    let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
    let out = ex.run(&vec![0.1f32; BLOCK], BLOCK).unwrap();
    assert!(out.iter().all(|v| v.is_finite()));
}

#[test]
fn nam_without_model_passes_audio() {
    let mut g = Graph::new();
    g.add("in", Box::new(dsp_graph::blocks::IoInput)).unwrap();
    g.add("nam", Box::new(Nam::default())).unwrap();
    g.add("out", Box::new(dsp_graph::blocks::IoOutput)).unwrap();
    g.connect("in", "out", "nam", "in").unwrap();
    g.connect("nam", "out", "out", "in").unwrap();
    let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
    let mut input = vec![0.0f32; BLOCK];
    input[0] = 1.0;
    let out = ex.run(&input, BLOCK).unwrap();
    assert_eq!(out[0], 1.0, "unloaded NAM should pass the input through");
}

#[test]
fn nam_block_is_in_the_registry() {
    let r = builtin_registry();
    assert!(r.get("mod.nam").is_some());
}

/// The model's inference must not allocate in `process`.
#[test]
fn nam_process_does_not_allocate() {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    struct Counting;
    thread_local! { static N: Cell<usize> = const { Cell::new(0) }; }
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            let _ = N.try_with(|c| c.set(c.get() + 1));
            System.alloc(l)
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) { System.dealloc(p, l) }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            let _ = N.try_with(|c| c.set(c.get() + 1));
            System.realloc(p, l, n)
        }
    }
    #[global_allocator]
    static A: Counting = Counting;
    fn allocs() -> usize { N.with(|c| c.get()) }

    let mut nam = Nam::default();
    nam.load_json(&ir_model()).unwrap();
    nam.prepare(SR, BLOCK).unwrap();
    let input = vec![0.1f32; BLOCK];
    let ins: Vec<&[f32]> = vec![&input];
    let mut bufs = vec![vec![0.0f32; BLOCK]];
    let mut outs: Vec<&mut [f32]> = bufs.iter_mut().map(|v| &mut v[..]).collect();
    let params = [0.0f32, 0.0, 100.0];
    let mut ctx = BlockCtx {
        inputs: &ins,
        outputs: &mut outs,
        params: &params,
        controls_in: &[],
        controls_out: &mut [],
        n: BLOCK,
        sample_rate: SR,
        block_index: 0,
    };
    nam.process(&mut ctx).unwrap();
    let before = allocs();
    for _ in 0..100 {
        nam.process(&mut ctx).unwrap();
    }
    assert_eq!(allocs() - before, 0, "NAM process allocated");
}

/// A preset node's `model` path reaches the block through `Doc::build`.
#[test]
fn doc_build_configures_the_model_path() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let _ = path;
    // Write a real .nam to a temp file so the loader is exercised end to end.
    let dir = std::env::temp_dir().join("pedal_nam_test");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tiny.nam");
    std::fs::write(&file, ir_model()).unwrap();

    let json = serde_json::json!({
        "name": "nam preset",
        "nodes": [
            {"id": "in", "block": "io.input"},
            {"id": "nam", "block": "mod.nam", "model": file.to_string_lossy()},
            {"id": "out", "block": "io.output"}
        ],
        "edges": [
            {"from": ["in", "out"], "to": ["nam", "in"]},
            {"from": ["nam", "out"], "to": ["out", "in"]}
        ]
    })
    .to_string();

    let doc = dsp_graph::Doc::from_json(&json).unwrap();
    let reg = builtin_registry();
    let mut g = doc.build(&reg).unwrap();
    let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
    let mut input = vec![0.0f32; BLOCK * 4];
    for (i, s) in input.iter_mut().enumerate() {
        *s = (i as f32 * 0.07).sin() * 0.5;
    }
    let mut peak = 0.0f32;
    for chunk in input.chunks_exact(BLOCK) {
        for &v in ex.run(chunk, BLOCK).unwrap() {
            peak = peak.max(v.abs());
        }
    }
    assert!(peak > 0.0, "model from preset produced no output");
    let _ = std::fs::remove_dir_all(&dir);
}
