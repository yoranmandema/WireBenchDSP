//! `pedal-ffi` — the C ABI between the Rust DSP core and a native plugin shell.
//!
//! The VST3 shell (iPlug2, C++) cannot call Rust directly, so the DSP core is
//! exposed through this flat C boundary. The shell links the `staticlib` and
//! includes `include/pedal_ffi.h`.
//!
//! The boundary is deliberately thin: one opaque handle, one preset load, one
//! parameter setter/getter, and one `process` call per block. Everything else
//! (the graph, the executor, the blocks) stays in `dsp-graph`.
//!
//! # Conventions
//!
//! * Parameters are **normalised 0..1** on the wire, like VST3 automation.
//!   `pedal_core_set_param_real` is the exception, for preset loading.
//! * Audio is **mono inside the graph** (the `io.input` / `io.output` ports are
//!   mono). A stereo host passes interleaved stereo and gets the mono signal
//!   duplicated to both channels. True stereo is a later story.
//! * `process` never allocates: the stereo deinterleave uses a scratch buffer
//!   allocated once in [`pedal_core_new`].
//! * Nothing here panics across the boundary. Every entry point checks its
//!   pointers and reports failure through [`pedal_core_last_error`].
//!
//! # Verification
//!
//! `cargo test -p pedal-ffi` checks the ABI from Rust;
//! `ffi/test-c.sh` compiles and runs a C harness against the real staticlib,
//! which is what the plugin shell will link.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

use dsp_graph::blocks::builtin_registry;
use dsp_graph::{Doc, Executor, Registry};

/// Success.
pub const PEDAL_OK: i32 = 0;
/// Failure. [`pedal_core_last_error`] carries the message.
pub const PEDAL_ERR: i32 = -1;

/// A compiled rig plus the state a native host needs.
///
/// Opaque to C: the shell only ever holds a `*mut PedalCore`.
pub struct PedalCore {
    sample_rate: f32,
    max_block: usize,
    registry: Registry,
    executor: Option<Executor>,
    /// The preset JSON, kept so a sample-rate change can recompile.
    preset: Option<String>,
    /// Mono scratch for stereo hosts, pre-allocated so `process` never allocates.
    scratch: Vec<f32>,
    last_error: CString,
}

impl PedalCore {
    /// Parse, build, and compile a preset. On success the executor is installed
    /// and the JSON retained.
    fn compile_text(&mut self, text: &str) -> Result<(), String> {
        let doc = Doc::from_json(text).map_err(|e| e.to_string())?;
        let mut graph = doc.build(&self.registry).map_err(|e| e.to_string())?;
        let executor = Executor::compile_owned(&mut graph, self.sample_rate, self.max_block)
            .map_err(|e| e.to_string())?;
        self.executor = Some(executor);
        self.preset = Some(text.to_string());
        Ok(())
    }

    fn set_error(&mut self, msg: &str) {
        self.last_error = CString::new(msg).unwrap_or_default();
    }
}

/// Borrow a `*mut PedalCore` as `Option<&mut PedalCore>`.
unsafe fn core_mut<'a>(core: *mut PedalCore) -> Option<&'a mut PedalCore> {
    core.as_mut()
}

/// Borrow a `*const c_char` as `Option<&str>`, rejecting null and non-UTF-8.
unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

/// Create a core. `sample_rate` is the host rate; `max_block` is the largest
/// block the host will ever ask for (VST3 declares it up front).
///
/// Returns null only on allocation failure.
#[no_mangle]
pub extern "C" fn pedal_core_new(sample_rate: f32, max_block: u32) -> *mut PedalCore {
    let max_block = max_block.max(1) as usize;
    let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    };
    Box::into_raw(Box::new(PedalCore {
        sample_rate,
        max_block,
        registry: builtin_registry(),
        executor: None,
        preset: None,
        scratch: vec![0.0; max_block],
        last_error: CString::default(),
    }))
}

/// Destroy a core. Null is accepted.
#[no_mangle]
pub extern "C" fn pedal_core_free(core: *mut PedalCore) {
    if !core.is_null() {
        drop(unsafe { Box::from_raw(core) });
    }
}

/// The last error message, or an empty string. The pointer is owned by the core
/// and stays valid until the next fallible call (or `pedal_core_free`).
#[no_mangle]
pub extern "C" fn pedal_core_last_error(core: *const PedalCore) -> *const c_char {
    match unsafe { core.as_ref() } {
        Some(core) => core.last_error.as_ptr(),
        None => b"\0".as_ptr().cast(),
    }
}

/// Load a preset from JSON text. Returns `PEDAL_OK` / `PEDAL_ERR`.
#[no_mangle]
pub extern "C" fn pedal_core_load_preset(core: *mut PedalCore, json: *const c_char) -> i32 {
    let Some(core) = (unsafe { core_mut(core) }) else {
        return PEDAL_ERR;
    };
    let Some(text) = (unsafe { cstr(json) }) else {
        core.set_error("preset is null or not valid UTF-8");
        return PEDAL_ERR;
    };
    match core.compile_text(text) {
        Ok(()) => PEDAL_OK,
        Err(msg) => {
            core.set_error(&msg);
            PEDAL_ERR
        }
    }
}

/// Load a preset from a file path. Relative `model` paths resolve against the
/// process working directory (the same rule as `Doc::build`).
#[no_mangle]
pub extern "C" fn pedal_core_load_preset_file(core: *mut PedalCore, path: *const c_char) -> i32 {
    let Some(core) = (unsafe { core_mut(core) }) else {
        return PEDAL_ERR;
    };
    let Some(path) = (unsafe { cstr(path) }) else {
        core.set_error("path is null or not valid UTF-8");
        return PEDAL_ERR;
    };
    match std::fs::read_to_string(path) {
        Ok(text) => match core.compile_text(&text) {
            Ok(()) => PEDAL_OK,
            Err(msg) => {
                core.set_error(&msg);
                PEDAL_ERR
            }
        },
        Err(e) => {
            core.set_error(&format!("cannot read {path}: {e}"));
            PEDAL_ERR
        }
    }
}

/// Change the sample rate. Recompiles the loaded preset, so a host that
/// reconfigures mid-session keeps its rig.
#[no_mangle]
pub extern "C" fn pedal_core_set_sample_rate(core: *mut PedalCore, sample_rate: f32) -> i32 {
    let Some(core) = (unsafe { core_mut(core) }) else {
        return PEDAL_ERR;
    };
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        core.set_error("sample rate must be positive");
        return PEDAL_ERR;
    }
    core.sample_rate = sample_rate;
    match core.preset.clone() {
        Some(text) => match core.compile_text(&text) {
            Ok(()) => PEDAL_OK,
            Err(msg) => {
                core.set_error(&msg);
                PEDAL_ERR
            }
        },
        None => PEDAL_OK,
    }
}

/// Set a parameter to a normalised value (0..1), like VST3 automation.
#[no_mangle]
pub extern "C" fn pedal_core_set_param_norm(
    core: *mut PedalCore,
    node: *const c_char,
    param: *const c_char,
    norm: f32,
) -> i32 {
    let Some(core) = (unsafe { core_mut(core) }) else {
        return PEDAL_ERR;
    };
    let (node, param) = match (unsafe { cstr(node) }, unsafe { cstr(param) }) {
        (Some(n), Some(p)) => (n, p),
        _ => {
            core.set_error("node or param is null or not valid UTF-8");
            return PEDAL_ERR;
        }
    };
    let result = core
        .executor
        .as_mut()
        .ok_or_else(|| "no preset loaded".to_string())
        .and_then(|ex| {
            ex.set_param_norm(node, param, norm)
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(()) => PEDAL_OK,
        Err(msg) => {
            core.set_error(&msg);
            PEDAL_ERR
        }
    }
}

/// Set a parameter to a real value (dB, Hz, ms, …), denormalised by the spec.
#[no_mangle]
pub extern "C" fn pedal_core_set_param_real(
    core: *mut PedalCore,
    node: *const c_char,
    param: *const c_char,
    real: f32,
) -> i32 {
    let Some(core) = (unsafe { core_mut(core) }) else {
        return PEDAL_ERR;
    };
    let (node, param) = match (unsafe { cstr(node) }, unsafe { cstr(param) }) {
        (Some(n), Some(p)) => (n, p),
        _ => {
            core.set_error("node or param is null or not valid UTF-8");
            return PEDAL_ERR;
        }
    };
    let result = core
        .executor
        .as_mut()
        .ok_or_else(|| "no preset loaded".to_string())
        .and_then(|ex| {
            ex.set_param_real(node, param, real)
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(()) => PEDAL_OK,
        Err(msg) => {
            core.set_error(&msg);
            PEDAL_ERR
        }
    }
}

/// Read a parameter's normalised value. Writes to `out` if it is non-null.
#[no_mangle]
pub extern "C" fn pedal_core_get_param_norm(
    core: *mut PedalCore,
    node: *const c_char,
    param: *const c_char,
    out: *mut f32,
) -> i32 {
    let Some(core) = (unsafe { core_mut(core) }) else {
        return PEDAL_ERR;
    };
    let (node, param) = match (unsafe { cstr(node) }, unsafe { cstr(param) }) {
        (Some(n), Some(p)) => (n, p),
        _ => {
            core.set_error("node or param is null or not valid UTF-8");
            return PEDAL_ERR;
        }
    };
    match core
        .executor
        .as_ref()
        .and_then(|ex| ex.param_norm(node, param))
    {
        Some(v) => {
            if !out.is_null() {
                unsafe { *out = v };
            }
            PEDAL_OK
        }
        None => {
            core.set_error("unknown node or param");
            PEDAL_ERR
        }
    }
}

/// Read a parameter's real value (dB, Hz, ms, …). Writes to `out` if non-null.
#[no_mangle]
pub extern "C" fn pedal_core_get_param_real(
    core: *mut PedalCore,
    node: *const c_char,
    param: *const c_char,
    out: *mut f32,
) -> i32 {
    let Some(core) = (unsafe { core_mut(core) }) else {
        return PEDAL_ERR;
    };
    let (node, param) = match (unsafe { cstr(node) }, unsafe { cstr(param) }) {
        (Some(n), Some(p)) => (n, p),
        _ => {
            core.set_error("node or param is null or not valid UTF-8");
            return PEDAL_ERR;
        }
    };
    match core
        .executor
        .as_ref()
        .and_then(|ex| ex.param_real(node, param))
    {
        Some(v) => {
            if !out.is_null() {
                unsafe { *out = v };
            }
            PEDAL_OK
        }
        None => {
            core.set_error("unknown node or param");
            PEDAL_ERR
        }
    }
}

/// Bypass a node: its first input is copied to every output.
#[no_mangle]
pub extern "C" fn pedal_core_set_bypass(core: *mut PedalCore, node: *const c_char, on: i32) -> i32 {
    let Some(core) = (unsafe { core_mut(core) }) else {
        return PEDAL_ERR;
    };
    let Some(node) = (unsafe { cstr(node) }) else {
        core.set_error("node is null or not valid UTF-8");
        return PEDAL_ERR;
    };
    let index = core.executor.as_ref().and_then(|ex| ex.node_index(node));
    let Some(index) = index else {
        core.set_error("unknown node");
        return PEDAL_ERR;
    };
    let applied = match core.executor.as_mut() {
        Some(ex) => ex.set_bypass_at(index, on != 0),
        None => false,
    };
    if applied {
        PEDAL_OK
    } else {
        core.set_error("unknown node");
        PEDAL_ERR
    }
}

/// Process one block.
///
/// `input` and `output` are interleaved, `n` frames each. `channels` must be
/// 1 or 2; the graph is mono, so a stereo host gets the same signal on both
/// channels (the left input channel is used).
///
/// Returns `PEDAL_ERR` if no preset is loaded or `n` exceeds the `max_block`
/// given to [`pedal_core_new`] — a host must not read unwritten output.
#[no_mangle]
pub extern "C" fn pedal_core_process(
    core: *mut PedalCore,
    input: *const f32,
    output: *mut f32,
    n: u32,
    channels: u32,
) -> i32 {
    let Some(core) = (unsafe { core_mut(core) }) else {
        return PEDAL_ERR;
    };
    if input.is_null() || output.is_null() {
        core.set_error("null audio buffer");
        return PEDAL_ERR;
    }
    if channels != 1 && channels != 2 {
        core.set_error("only mono (1) and stereo (2) are supported");
        return PEDAL_ERR;
    }
    let n = n as usize;
    if n == 0 {
        return PEDAL_OK;
    }
    if n > core.max_block {
        core.set_error("block exceeds the configured maximum");
        return PEDAL_ERR;
    }

    let mono: &[f32] = if channels == 1 {
        unsafe { std::slice::from_raw_parts(input, n) }
    } else {
        for i in 0..n {
            core.scratch[i] = unsafe { *input.add(i * 2) };
        }
        &core.scratch[..n]
    };

    let out = unsafe { std::slice::from_raw_parts_mut(output, n * channels as usize) };

    let Some(executor) = core.executor.as_mut() else {
        core.set_error("no preset loaded");
        return PEDAL_ERR;
    };

    let mut error: Option<String> = None;
    match executor.run(mono, n) {
        Ok(buf) => {
            if channels == 1 {
                out.copy_from_slice(buf);
            } else {
                for (i, &s) in buf.iter().enumerate() {
                    out[2 * i] = s;
                    out[2 * i + 1] = s;
                }
            }
        }
        Err(e) => error = Some(e.to_string()),
    }

    if let Some(msg) = error {
        core.set_error(&msg);
        return PEDAL_ERR;
    }
    PEDAL_OK
}

/// Clear all node and delay state. Called on preset change and on bypass.
#[no_mangle]
pub extern "C" fn pedal_core_reset(core: *mut PedalCore) {
    if let Some(core) = unsafe { core_mut(core) } {
        if let Some(executor) = core.executor.as_mut() {
            executor.reset();
        }
    }
}

/// The graph's total algorithmic latency in samples.
#[no_mangle]
pub extern "C" fn pedal_core_latency(core: *const PedalCore) -> u32 {
    match unsafe { core.as_ref() } {
        Some(core) => core
            .executor
            .as_ref()
            .map(|ex| ex.total_latency())
            .unwrap_or(0),
        None => 0,
    }
}

/// Call `cb` for every node id, in executor order.
#[no_mangle]
pub extern "C" fn pedal_core_foreach_node(
    core: *mut PedalCore,
    cb: Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
    userdata: *mut c_void,
) {
    let Some(cb) = cb else { return };
    let Some(core) = (unsafe { core_mut(core) }) else {
        return;
    };
    let Some(executor) = core.executor.as_ref() else {
        return;
    };
    for id in executor.node_ids() {
        if let Ok(c) = CString::new(id.as_str()) {
            unsafe { cb(userdata, c.as_ptr()) };
        }
    }
}

/// Call `cb` for every parameter of `node`, with its current normalised value.
#[no_mangle]
pub extern "C" fn pedal_core_foreach_param(
    core: *mut PedalCore,
    node: *const c_char,
    cb: Option<unsafe extern "C" fn(*mut c_void, *const c_char, f32)>,
    userdata: *mut c_void,
) {
    let Some(cb) = cb else { return };
    let Some(core) = (unsafe { core_mut(core) }) else {
        return;
    };
    let Some(node) = (unsafe { cstr(node) }) else {
        return;
    };
    let Some(executor) = core.executor.as_ref() else {
        return;
    };
    let Some(index) = executor.node_index(node) else {
        return;
    };
    let params: Vec<&'static str> = executor.specs()[index]
        .params
        .iter()
        .map(|p| p.id)
        .collect();
    for param in params {
        let norm = executor.param_norm(node, param).unwrap_or(0.0);
        if let Ok(c) = CString::new(param) {
            unsafe { cb(userdata, c.as_ptr(), norm) };
        }
    }
}
