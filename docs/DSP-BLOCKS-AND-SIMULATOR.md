# DSP Blocks, Host Simulator, and Cross-Platform Development

Status: draft v0.1 — design before code.
Depends on: `product-model.md`, `executor.md`.
New requirement: **everything runs on the user's Mac** (Apple Silicon, macOS)
for development and testing. No Pi needed until deployment.

---

## 0. The consequence of Mac-first development

This is the most important architectural constraint, and it's a good one.

**Rule: the DSP and graph crates have no platform dependencies.** No ALSA, no
CoreAudio, no file paths, no `std::process`. They compile to macOS, Linux, and
the Pi from the same source.

What follows:

| Crate | Platform deps | Runs on Mac | Runs on Pi |
|---|---|---|---|
| `dsp-core` | none | yes | yes |
| `dsp-graph` | none | yes | yes |
| `dsp-blocks` | none | yes | yes |
| `dsp-nam` | none | yes | yes |
| `host-sim` | WAV I/O only | yes | yes |
| `audio` | ALSA / CoreAudio | CoreAudio | ALSA |
| `app` | audio + UI | yes | yes |

The **only** platform-specific code is the audio backend, and it sits behind a
trait. Everything above it is shared.

This means:
- `cargo test` on the Mac covers 100% of the DSP.
- The graph JSON is identical on both platforms.
- The host simulator renders the same WAV on both.
- Deployment to the Pi is a cross-compile, not a port.

---

## 1. Workspace layout

```
pedal/
  Cargo.toml                 # workspace
  crates/
    dsp-core/                # biquad, envelope, delay line, FFT, wav helpers
    dsp-graph/               # Block trait, Graph, compile pipeline, Executor
    dsp-blocks/              # the effect library
    dsp-nam/                 # NAM model loading + inference
    host-sim/                # WAV in → graph → WAV out (binary)
    audio/                   # AudioBackend trait
    audio-alsa/              # Linux implementation
    audio-coreaudio/         # macOS implementation
    app/                     # live app + UI
```

Dependencies flow one way: `app → audio → dsp-graph → dsp-blocks → dsp-core`.
Nothing flows back up.

---

## 2. The Block trait

The whole plugin architecture is this trait. It's deliberately small — an
author should implement it in an afternoon.

```rust
pub trait Block: Send {
    /// Static description: identity, ports, params. Built once.
    fn spec(&self) -> BlockSpec;

    /// Allocate and compute coefficients. Called at compile time, never in the
    /// audio callback. This is where IRs are loaded and models are parsed.
    fn prepare(&mut self, sr: f32, max_block: usize) -> Result<(), BlockError>;

    /// Process one block. Must not allocate, must not panic, must not block.
    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError>;

    /// Clear all internal state. Called on preset change and on bypass.
    fn reset(&mut self);

    /// Algorithmic delay in samples. Used by the latency pass.
    /// Default 0 — a causal effect declares nothing.
    fn latency_samples(&self) -> u32 { 0 }
}
```

`Send` (not `Sync`): a block is created on the UI thread, prepared, then moved
to the audio thread. It is never shared between threads.

### 2.1 BlockCtx

```rust
pub struct BlockCtx<'a> {
    /// Audio inputs, one slice per input port, `ctx.n` frames each.
    pub inputs: &'a [&'a [f32]],
    /// Audio outputs, one slice per output port, `ctx.n` frames each.
    pub outputs: &'a mut [&'a mut [f32]],
    /// Smoothed parameter values, indexed by param id.
    pub params: &'a Params,
    /// Control input values (LFO, envelope, expression pedal).
    pub controls_in: &'a [f32],
    /// Control output values (envelope follower, gate).
    pub controls_out: &'a mut [f32],
    /// Frames in this block.
    pub n: usize,
    pub sample_rate: f32,
    pub block_index: u64,
}
```

Slices, not `Vec`. The executor hands the block views into its pre-allocated
buffer pool. The block never allocates.

---

## 3. Specs

```rust
pub struct BlockSpec {
    pub id: &'static str,              // "eq.parametric"
    pub name: &'static str,            // "Parametric EQ"
    pub category: Category,            // Dynamics, Tone, Modulation, ...
    pub inputs: Vec<PortSpec>,
    pub outputs: Vec<PortSpec>,
    pub params: Vec<ParamSpec>,
    pub controls_in: Vec<ControlSpec>,
    pub controls_out: Vec<ControlSpec>,
}

pub struct PortSpec {
    pub name: &'static str,
    pub kind: PortKind,                // Audio | Control | Gate
}

pub struct ParamSpec {
    pub id: &'static str,              // "gain"
    pub name: &'static str,            // "Gain"
    pub unit: Unit,                    // dB | Hz | ms | Percent | Semitones | Cents | Ratio | Linear
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub curve: Curve,                  // Linear | Log | AmpGain | ...
}
```

The UI builds its knob strip from `params`. The MIDI learn system maps from
`params`. The serialiser stores `params`. One declaration, three consumers.

---

## 4. The template

This is what makes "a new effect in an afternoon" real. A complete block:

```rust
use dsp_graph::prelude::*;
use dsp_core::biquad::Biquad;

pub struct ParametricEq {
    bands: Vec<Biquad>,
}

impl Default for ParametricEq {
    fn default() -> Self {
        Self { bands: Vec::new() }
    }
}

impl Block for ParametricEq {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "eq.parametric",
            name: "Parametric EQ",
            category: Category::Tone,
            inputs:  vec![PortSpec { name: "in",  kind: PortKind::Audio }],
            outputs: vec![PortSpec { name: "out", kind: PortKind::Audio }],
            params: vec![
                ParamSpec { id: "low_gain",  name: "Low",  unit: Unit::Decibels,
                            min: -15.0, max: 15.0, default: 0.0,
                            curve: Curve::Linear },
                ParamSpec { id: "mid_gain",  name: "Mid",  unit: Unit::Decibels,
                            min: -15.0, max: 15.0, default: 0.0,
                            curve: Curve::Linear },
                ParamSpec { id: "high_gain", name: "High", unit: Unit::Decibels,
                            min: -15.0, max: 15.0, default: 0.0,
                            curve: Curve::Linear },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, sr: f32, _max_block: usize) -> Result<(), BlockError> {
        self.bands = vec![
            Biquad::low_shelf(sr, 200.0, 0.7),
            Biquad::peaking(sr, 1000.0, 1.0),
            Biquad::high_shelf(sr, 4000.0, 0.7),
        ];
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        let input = ctx.inputs[0];
        let output = &mut ctx.outputs[0][..];
        let gains = [
            ctx.params.get_db("low_gain"),
            ctx.params.get_db("mid_gain"),
            ctx.params.get_db("high_gain"),
        ];
        // set gains, then run the cascade
        for (band, g) in self.bands.iter_mut().zip(gains) {
            band.set_gain(g);
        }
        for i in 0..ctx.n {
            let mut s = input[i];
            for band in self.bands.iter_mut() {
                s = band.process_sample(s);
            }
            output[i] = s;
        }
        Ok(())
    }

    fn reset(&mut self) {
        for band in &mut self.bands { band.reset(); }
    }
}
```

**The author wrote ~25 lines of DSP.** The boilerplate (spec, params, UI,
serialisation, MIDI, registry) is either declarative or generated.

### 4.1 Scaffolding

A template command copies a skeleton and asks for the name:

```
cargo scaffold my_effect
```

produces `crates/dsp-blocks/src/my_effect.rs` with the spec and a `todo!()` in
`process`, plus a test file. The author fills in `process` and the params.

This is a small tool, but it's the difference between "I'll add that later" and
"I added it this afternoon".

---

## 5. Registration

Blocks self-register, so adding a file adds an effect:

```rust
// in dsp-blocks/src/lib.rs
pub fn registry() -> Registry {
    let mut r = Registry::new();
    r.add::<gain::Gain>();
    r.add::<eq::ParametricEq>();
    r.add::<compressor::Compressor>();
    r.add::<cab::CabIr>();
    r.add::<nam::NamAmp>();
    // ...
    r
}
```

A central list is fine for v1 — it's one line per block, and it makes the
dependency graph explicit. (Later, `inventory`-style auto-registration removes
even that.)

The registry is what the UI's palette, the JSON loader, and the validator all
use. One source of truth.

---

## 6. Host simulator

The simulator is the development environment. It runs the **same** executor
code as the Pi.

### 6.1 CLI

```
host-sim --graph rig.json --in input.wav --out output.wav
host-sim --graph rig.json --ir                  # print impulse response
host-sim --graph rig.json --bench               # cycles/block, CPU budget
host-sim --graph rig.json --spectrum out.wav    # FFT of the output
host-sim --graph rig.json --watch --in in.wav --out out.wav
host-sim --compare a.json b.json --in in.wav    # A/B two graphs
```

### 6.2 Graph JSON

The same format the Pi loads:

```json
{
  "sample_rate": 48000,
  "block_size": 64,
  "nodes": [
    { "id": "in",    "type": "io.input" },
    { "id": "amp",   "type": "nam.amp",  "model": "marshally.nam",
                 "params": { "gain": 0.7, "presence": 0.4 } },
    { "id": "cab",   "type": "cab.ir",   "file": "cab.wav" },
    { "id": "out",   "type": "io.output" }
  ],
  "edges": [
    { "from": ["in", "out"],  "to": ["amp", "in"] },
    { "from": ["amp", "out"], "to": ["cab", "in"] },
    { "from": ["cab", "out"], "to": ["out", "in"] }
  ]
}
```

Because it's JSON, you can edit it in a text editor and re-render instantly.
That's the fast iteration loop.

### 6.3 `--watch` mode

Watches the graph JSON (and any IR/model files it references) and re-renders the
WAV when they change. Edit a knob → save → hear it. This is the "vibe code"
loop: no compile step for the graph, only for new blocks.

### 6.4 Measurements

The simulator reports:
- peak and RMS of the output,
- cycles per block (and % of the 50,000-cycle budget),
- the graph's total algorithmic latency,
- a stability check (output finite, no NaN, no runaway),
- an impulse response for latency verification.

These are the same numbers the Pi will report, so a graph that benches well on
the Mac will bench well on the Pi.

---

## 7. Audio backend

```rust
pub trait AudioBackend {
    fn open(config: &AudioConfig) -> Result<Self, AudioError>
        where Self: Sized;
    fn start(&mut self, callback: Box<dyn AudioCallback + Send>)
        -> Result<(), AudioError>;
    fn stop(&mut self) -> Result<(), AudioError>;
}

pub trait AudioCallback {
    /// Called once per period with `n` frames of input; fill `output`.
    fn process(&mut self, input: &[f32], output: &mut [f32], n: usize);
}
```

Two implementations:

| Backend | Platform | Notes |
|---|---|---|
| `audio-alsa` | Linux / Pi | Direct `hw:` mmap, zero copy, 2–3 periods |
| `audio-coreaudio` | macOS | CoreAudio HAL, the Scarlett 2i2 as the device |

The executor is backend-agnostic. `AudioCallback::process` just calls
`executor.run()`.

**Why CoreAudio matters for development:** you have a Scarlett 2i2. Running the
engine live on the Mac — guitar in, processed sound out, through the same
interface you'll use on the Pi — means you can develop the *entire* product
before the Pi arrives. The DSP, the graph, the UI, the control bindings, all of
it.

---

## 8. The Mac development loop

```
1. Edit rig.json (or the UI)
2. host-sim --watch re-renders the WAV
3. Listen on headphones
4. Tweak
5. When the graph is right, run it live via CoreAudio
6. When the Pi arrives, cross-compile the same crates
```

No Pi, no SD card, no flashing, no serial console. The Mac is the development
machine and the reference platform.

**The Pi becomes a deployment target, not a development target.** That's a
much better position to be in.

---

## 9. Testing

| Level | What | Where |
|---|---|---|
| Block unit | Impulse, step, DC, stability, NaN, latency | `cargo test -p dsp-blocks` |
| Graph unit | Step order, buffer disjointness, latency compensation | `cargo test -p dsp-graph` |
| Golden | Render a WAV, compare to a reference | `host-sim` |
| Bench | Cycles per block for a representative graph | `host-sim --bench` |
| Stress | 1,000 hot-swaps, no xruns | `host-sim` |
| Live | Round-trip latency, xrun count | `app` on Mac and Pi |

All of these run on the Mac. The Pi only needs the live test.

---

## 10. Build order

1. **`dsp-core`** — biquad, gain, delay line, envelope. Pure functions, unit
   tested. No platform deps.
2. **`dsp-graph`** — Block trait, Graph, compile pipeline, Executor. The
   executor from `executor.md`, simplified: topological sort + buffer plan +
   flat step list. Latency pass comes next.
3. **`dsp-blocks`** — gain, EQ, compressor, cab IR, and a NAM stub. Enough to
   build a real rig.
4. **`host-sim`** — WAV I/O + JSON graph + CLI + `--bench`. This is the proof
   that the architecture works.
5. **The latency pass** — the phase-coherence mechanism from `executor.md`.
6. **NAM inference** — replace the stub with `nam-rs` or a hand-rolled A2.
7. **`audio-coreaudio`** — live development on the Mac.
8. **The UI** — web UI over a JSON API, then native if needed.
9. **`audio-alsa` + `app`** — the Pi deployment.

Steps 1–4 are the proof of the whole architecture. If they work, everything
after is additive.

---

## 11. Open details

1. **WAV I/O.** Use the `hound` crate (pure Rust, cross-platform) rather than
   hand-rolling a parser.
2. **FFT.** `realfft` or `rustfft` for the cab IR and spectrum analysis. Both
   are pure Rust.
3. **JSON.** `serde` + `serde_json`. Stable, fast, everywhere.
4. **Does the Mac need a real-time thread?** No — CoreAudio gives you a
   real-time callback already. The `SCHED_FIFO`/isolated-core work is Pi-only.
5. **Block spec as `&'static` vs owned.** Start with owned `Vec`s in the spec;
   it's built once. Optimise later if the palette is slow.
