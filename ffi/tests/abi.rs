//! The C ABI, exercised from Rust as if it were C.
//!
//! The tests declare the same `extern "C"` block a C host would use, so a
//! symbol that is missing, mis-typed, or renamed fails here. `test-c.sh` does
//! the same from a real C translation unit against the staticlib.

extern crate pedal_ffi;

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

const SR: f32 = 48_000.0;
const BLOCK: u32 = 64;

/// A rig with one parameter, so the param paths are easy to reason about.
const PRESET: &str = r#"{
  "name": "FFI test",
  "nodes": [
    { "id": "in", "block": "io.input" },
    { "id": "amp", "block": "amp.basic", "params": { "gain": 3.5 } },
    { "id": "out", "block": "io.output" }
  ],
  "edges": [
    { "from": ["in", "out"], "to": ["amp", "in"] },
    { "from": ["amp", "out"], "to": ["out", "in"] }
  ]
}"#;

extern "C" {
    fn pedal_core_new(sample_rate: f32, max_block: u32) -> *mut c_void;
    fn pedal_core_free(core: *mut c_void);
    fn pedal_core_last_error(core: *const c_void) -> *const c_char;
    fn pedal_core_load_preset(core: *mut c_void, json: *const c_char) -> i32;
    fn pedal_core_set_sample_rate(core: *mut c_void, sample_rate: f32) -> i32;
    fn pedal_core_set_param_norm(
        core: *mut c_void,
        node: *const c_char,
        param: *const c_char,
        norm: f32,
    ) -> i32;
    fn pedal_core_get_param_norm(
        core: *mut c_void,
        node: *const c_char,
        param: *const c_char,
        out: *mut f32,
    ) -> i32;
    fn pedal_core_set_param_real(
        core: *mut c_void,
        node: *const c_char,
        param: *const c_char,
        real: f32,
    ) -> i32;
    fn pedal_core_get_param_real(
        core: *mut c_void,
        node: *const c_char,
        param: *const c_char,
        out: *mut f32,
    ) -> i32;
    fn pedal_core_set_bypass(core: *mut c_void, node: *const c_char, on: i32) -> i32;
    fn pedal_core_process(
        core: *mut c_void,
        input: *const f32,
        output: *mut f32,
        n: u32,
        channels: u32,
    ) -> i32;
    fn pedal_core_reset(core: *mut c_void);
    fn pedal_core_latency(core: *const c_void) -> u32;
    fn pedal_core_foreach_node(
        core: *mut c_void,
        cb: Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
        userdata: *mut c_void,
    );
    fn pedal_core_foreach_param(
        core: *mut c_void,
        node: *const c_char,
        cb: Option<unsafe extern "C" fn(*mut c_void, *const c_char, f32)>,
        userdata: *mut c_void,
    );
}

/// A 220 Hz sine, the same signal the Python reference uses.
fn sine(n: usize, from: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.2 * (2.0 * std::f32::consts::PI * 220.0 * (from + i) as f32 / SR).sin())
        .collect()
}

fn last_error(core: *mut c_void) -> String {
    unsafe { CStr::from_ptr(pedal_core_last_error(core)) }
        .to_string_lossy()
        .into_owned()
}

#[test]
fn process_matches_a_direct_executor_run() {
    let core = unsafe { pedal_core_new(SR, BLOCK) };
    assert!(!core.is_null());
    let json = CString::new(PRESET).unwrap();
    assert_eq!(unsafe { pedal_core_load_preset(core, json.as_ptr()) }, 0);

    // The same rig, built directly through dsp-graph.
    let doc = dsp_graph::Doc::from_json(PRESET).unwrap();
    let mut graph = doc.build(&dsp_graph::blocks::builtin_registry()).unwrap();
    let mut direct = dsp_graph::Executor::compile_owned(&mut graph, SR, BLOCK as usize).unwrap();

    let mut total_error = 0.0f32;
    let mut peak = 0.0f32;
    for block in 0..8 {
        let input = sine(BLOCK as usize, block * BLOCK as usize);
        let mut out = vec![0.0f32; BLOCK as usize];
        assert_eq!(
            unsafe { pedal_core_process(core, input.as_ptr(), out.as_mut_ptr(), BLOCK, 1) },
            0
        );
        let expected = direct.run(&input, BLOCK as usize).unwrap();
        for (a, b) in out.iter().zip(expected) {
            total_error = total_error.max((a - b).abs());
            peak = peak.max(b.abs());
        }
    }
    assert!(peak > 0.01, "output is silent (peak {peak})");
    assert!(
        total_error < 1e-6,
        "ABI output differs from the executor: {total_error}"
    );

    // Latency is reported, not invented.
    assert!(unsafe { pedal_core_latency(core) } == direct.total_latency());

    unsafe { pedal_core_free(core) };
}

#[test]
fn stereo_duplicates_the_mono_graph() {
    let core = unsafe { pedal_core_new(SR, BLOCK) };
    let json = CString::new(PRESET).unwrap();
    assert_eq!(unsafe { pedal_core_load_preset(core, json.as_ptr()) }, 0);

    let input = sine(BLOCK as usize, 0);
    let mut stereo = vec![0.0f32; (BLOCK * 2) as usize];
    for (i, &s) in input.iter().enumerate() {
        stereo[2 * i] = s;
        stereo[2 * i + 1] = s;
    }
    let mut out = vec![0.0f32; (BLOCK * 2) as usize];
    assert_eq!(
        unsafe { pedal_core_process(core, stereo.as_ptr(), out.as_mut_ptr(), BLOCK, 2) },
        0
    );
    for i in 0..BLOCK as usize {
        assert_eq!(out[2 * i], out[2 * i + 1], "channel {i} diverged");
    }
    unsafe { pedal_core_free(core) };
}

#[test]
fn parameters_round_trip_in_normalised_and_real_space() {
    let core = unsafe { pedal_core_new(SR, BLOCK) };
    let json = CString::new(PRESET).unwrap();
    assert_eq!(unsafe { pedal_core_load_preset(core, json.as_ptr()) }, 0);

    let node = CString::new("amp").unwrap();
    let param = CString::new("gain").unwrap();
    let mut norm = 0.0f32;
    assert_eq!(
        unsafe { pedal_core_get_param_norm(core, node.as_ptr(), param.as_ptr(), &mut norm) },
        0
    );
    assert!((0.0..=1.0).contains(&norm));

    assert_eq!(
        unsafe { pedal_core_set_param_norm(core, node.as_ptr(), param.as_ptr(), 0.25) },
        0
    );
    assert_eq!(
        unsafe { pedal_core_get_param_norm(core, node.as_ptr(), param.as_ptr(), &mut norm) },
        0
    );
    assert!((norm - 0.25).abs() < 1e-6, "norm {norm}");

    let mut real = 0.0f32;
    assert_eq!(
        unsafe { pedal_core_get_param_real(core, node.as_ptr(), param.as_ptr(), &mut real) },
        0
    );
    assert!(real.is_finite());
    assert_eq!(
        unsafe { pedal_core_set_param_real(core, node.as_ptr(), param.as_ptr(), real) },
        0
    );
    assert_eq!(
        unsafe { pedal_core_get_param_norm(core, node.as_ptr(), param.as_ptr(), &mut norm) },
        0
    );
    assert!(
        (norm - 0.25).abs() < 1e-3,
        "real round trip drifted: {norm}"
    );

    // Unknown names are errors, not panics.
    let bogus = CString::new("nope").unwrap();
    assert_eq!(
        unsafe { pedal_core_get_param_norm(core, bogus.as_ptr(), param.as_ptr(), &mut norm) },
        -1
    );
    assert!(!last_error(core).is_empty());

    unsafe { pedal_core_free(core) };
}

#[test]
fn bypass_and_reset_are_reachable() {
    let core = unsafe { pedal_core_new(SR, BLOCK) };
    let json = CString::new(PRESET).unwrap();
    assert_eq!(unsafe { pedal_core_load_preset(core, json.as_ptr()) }, 0);

    let node = CString::new("amp").unwrap();
    assert_eq!(unsafe { pedal_core_set_bypass(core, node.as_ptr(), 1) }, 0);
    let input = sine(BLOCK as usize, 0);
    let mut out = vec![0.0f32; BLOCK as usize];
    assert_eq!(
        unsafe { pedal_core_process(core, input.as_ptr(), out.as_mut_ptr(), BLOCK, 1) },
        0
    );
    // Bypassed: the amp's first input is copied to its output, so the graph
    // output equals the input (the master node is absent in PRESET).
    for (a, b) in out.iter().zip(&input) {
        assert!((a - b).abs() < 1e-6, "bypass changed the signal");
    }

    unsafe { pedal_core_reset(core) };
    assert_eq!(unsafe { pedal_core_set_bypass(core, node.as_ptr(), 0) }, 0);
    unsafe { pedal_core_free(core) };
}

#[test]
fn enumeration_walks_nodes_and_params() {
    let core = unsafe { pedal_core_new(SR, BLOCK) };
    let json = CString::new(PRESET).unwrap();
    assert_eq!(unsafe { pedal_core_load_preset(core, json.as_ptr()) }, 0);

    let nodes = std::sync::Mutex::new(Vec::<String>::new());
    let params = std::sync::Mutex::new(Vec::<String>::new());

    unsafe extern "C" fn on_node(userdata: *mut c_void, id: *const c_char) {
        let bag = &*(userdata as *const std::sync::Mutex<Vec<String>>);
        bag.lock()
            .unwrap()
            .push(CStr::from_ptr(id).to_string_lossy().into_owned());
    }
    unsafe extern "C" fn on_param(userdata: *mut c_void, id: *const c_char, _norm: f32) {
        let bag = &*(userdata as *const std::sync::Mutex<Vec<String>>);
        bag.lock()
            .unwrap()
            .push(CStr::from_ptr(id).to_string_lossy().into_owned());
    }

    unsafe { pedal_core_foreach_node(core, Some(on_node), &nodes as *const _ as *mut c_void) };
    let nodes = nodes.into_inner().unwrap();
    assert_eq!(
        nodes,
        vec!["in", "amp", "out"],
        "every node, in executor order"
    );

    let amp = CString::new("amp").unwrap();
    unsafe {
        pedal_core_foreach_param(
            core,
            amp.as_ptr(),
            Some(on_param),
            &params as *const _ as *mut c_void,
        )
    };
    let params = params.into_inner().unwrap();
    assert!(!params.is_empty(), "amp declares parameters");
    assert!(params.iter().all(|p: &String| !p.is_empty()));

    unsafe { pedal_core_free(core) };
}

#[test]
fn changing_the_sample_rate_recompiles_the_rig() {
    let core = unsafe { pedal_core_new(SR, BLOCK) };
    let json = CString::new(PRESET).unwrap();
    assert_eq!(unsafe { pedal_core_load_preset(core, json.as_ptr()) }, 0);

    // The first block after a compile is a filter transient; measure the
    // steady state over the following blocks.
    let peak = |from: usize| {
        let mut peak = 0.0f32;
        for block in from..(from + 8) {
            let input = sine(BLOCK as usize, block * BLOCK as usize);
            let mut out = vec![0.0f32; BLOCK as usize];
            assert_eq!(
                unsafe { pedal_core_process(core, input.as_ptr(), out.as_mut_ptr(), BLOCK, 1) },
                0
            );
            peak = peak.max(out.iter().fold(0.0f32, |a, &b| a.max(b.abs())));
        }
        peak
    };

    let before = peak(0);
    assert!(before > 0.01, "rig is silent before the rate change");

    assert_eq!(unsafe { pedal_core_set_sample_rate(core, 44_100.0) }, 0);
    let after = peak(0);
    assert!(after > 0.01, "rig is silent after a rate change");

    // A nonsense rate is refused, and the rig is left as it was.
    assert_eq!(unsafe { pedal_core_set_sample_rate(core, -1.0) }, -1);
    let still = peak(0);
    assert!(still > 0.01, "rig is silent after a refused rate change");

    unsafe { pedal_core_free(core) };
}

#[test]
fn errors_are_reported_not_panics() {
    let core = unsafe { pedal_core_new(SR, BLOCK) };
    assert_eq!(unsafe { pedal_core_last_error(core) }.is_null(), false);

    let bad = CString::new("{ not json").unwrap();
    assert_eq!(unsafe { pedal_core_load_preset(core, bad.as_ptr()) }, -1);
    assert!(!last_error(core).is_empty());

    // No preset loaded: process fails cleanly.
    let input = sine(BLOCK as usize, 0);
    let mut out = vec![0.0f32; BLOCK as usize];
    assert_eq!(
        unsafe { pedal_core_process(core, input.as_ptr(), out.as_mut_ptr(), BLOCK, 1) },
        -1
    );
    assert!(!last_error(core).is_empty());

    // Block larger than the declared maximum is refused.
    let preset = CString::new(PRESET).unwrap();
    assert_eq!(unsafe { pedal_core_load_preset(core, preset.as_ptr()) }, 0);
    assert_eq!(
        unsafe { pedal_core_process(core, input.as_ptr(), out.as_mut_ptr(), BLOCK + 1, 1,) },
        -1
    );

    unsafe { pedal_core_free(core) };
    // Null pointers are accepted everywhere and do nothing.
    unsafe { pedal_core_free(std::ptr::null_mut()) };
    assert_eq!(
        unsafe { pedal_core_last_error(std::ptr::null()) }.is_null(),
        false
    );
}
