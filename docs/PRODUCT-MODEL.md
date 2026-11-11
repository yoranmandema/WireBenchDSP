# Product Model & Plugin Architecture

Status: draft v0.1 — design before code.
Scope: what a preset is, how signal flows, and how new effects get built.

---

## 1. What we are actually modelling

Three ideas do all the work. Everything else is a detail.

| Concept | What it is | Example |
|---|---|---|
| **Node** | One processing object with typed ports | A NAM model, a cab IR, a compressor, a reverb, a tuner |
| **Edge** | A directional audio connection: output port → input port | `NAM.Out → Comp.In` |
| **Document** | Nodes + edges + per-node settings, serialised as one preset | "MY MARSHALL RIG" |

That is the whole product. A preset is a graph. A rig is a graph. A bass
channel-split patch is a graph with two branches.

Everything else in this document is about making that graph cheap to build,
safe to run, and fast to extend.

---

## 2. The graph

### 2.1 Shape

The graph is **arbitrary and cyclic-tolerant by design**, not a fixed chain.

```
                    ┌──────────────┐
   Guitar In ──────►│ Split        │
                    │  L / H       │
                    └──────┬───────┘
                     ┌─────┴──────┐
                     ▼            ▼
              ┌───────────┐  ┌───────────┐
              │ Clean NAM │  │ Dist NAM  │
              └─────┬─────┘  └─────┬─────┘
                    │              │
                    ▼              ▼
              ┌───────────┐  ┌───────────┐
              │ Cab IR    │  │ PitchSh.  │
              └─────┬─────┘  └─────┬─────┘
                    │              │
                    └──────┬───────┘
                           ▼
                    ┌──────────────┐
                    │  Mix (sum)   │
                    └──────┬───────┘
                           ▼
                       Main Out
```

Any output can feed any input. Parallel branches, recombines, feedback loops,
a pitch shifter after everything — all of it is just nodes and edges.

### 2.2 The one constraint: latency compensation

This is the single most important thing in this document, and it is the reason
a "fully free-form graph" is not free.

**Problem.** Nodes have different algorithmic delays.

Latency is a property of the implementation, not the effect type. A causal FIR
(a cabinet IR, a direct convolution) has **zero** algorithmic delay — the output
at sample `t` needs only input `t` and earlier. The same IR implemented with
overlap-add FFT costs one block of delay, because a full block must be filled
before the transform runs.

- A NAM model is causal (its dilated convolutions look backward), so its
  algorithmic delay is ~0. An FFT/block implementation costs a block.
- A cabinet IR in direct form: ~0. In overlap-add FFT form: ~1 block.
- A look-ahead compressor or limiter: deliberate, declared delay.
- A phase vocoder pitch shifter: ~1–3 blocks.

**If two parallel branches have different delays, they recombine out of phase.**
On a guitar, a clean sub branch and a distorted high branch that are 2 ms apart
comb-filter each other. The low end thins out, the tone gets hollow, and the
parallel blend sounds like a mistake. This is the classic parallel-compression
and parallel-amp problem, and commercial modelers spend real engineering on it.

**Rule. Every input port declares a latency (in samples). Every edge is
compensated.**

- Each node reports `latency_samples()` — how many samples later its output
  appears relative to its input.
- When a node has multiple input ports, the runtime aligns all inputs to the
  **maximum** declared latency among them, by delaying the earlier ones.
- The runtime then delays the *node's own* output by that same amount, so the
  node's total latency is `max(input latencies) + own latency`.
- The node itself always sees **time-aligned** inputs. The compensation
  happens outside the node, in the graph executor. Authors never think about it.

This makes "compressor after the amp but before the cab" free: the cab's
convolution delay is measured, the compressor is padded to match, and the two
recombine cleanly.

> This is the difference between a demo and a product. Without it, parallel
> paths sound broken. With it, the graph can be genuinely free-form.

**Practical note.** Latency is a whole number of samples. It is fixed at node
construction (or recomputed when a parameter that changes it is edited — e.g.
swapping a cab IR). The compensation buffers are allocated when the graph is
compiled, not in the audio callback.

---

## 3. Nodes

### 3.1 Ports are typed and semantic

Not just "audio in / audio out". Types let the UI build sensible cables and let
the runtime reject nonsense at compile time.

| Port kind | Meaning | Example |
|---|---|---|
| `Audio` | PCM audio | Every effect's main I/O |
| `Control` | A continuous parameter, sampled once per block | Expression pedal, LFO, envelope follower, sidechain signal |
| `Gate` | A boolean state, once per block | Bypass, mute, tuner switch, looper record |
| `Midi` | MIDI messages | Clock in, transport |
| `Event` | Discrete triggers | Tap tempo, preset change |

**Why this matters for creativity:** a `Control` output from an envelope
follower can feed a `Control` input on a filter's cutoff. A gate from a tuner
can mute the output. An LFO can drive a pitch shifter. Control routing is what
makes a pedalboard feel alive rather than like a fixed chain.

### 3.2 Node interface

A node is small and stateless from the graph's point of view:

```
trait Node {
    // Identity
    fn id(&self) -> NodeId;
    fn name(&self) -> &str;              // "NAM Amp", "Cab IR", "Comp"

    // Structure — static
    fn inputs(&self)  -> &[PortSpec];    // typed, named
    fn outputs(&self) -> &[PortSpec];

    // Behaviour
    fn latency_samples(&self, graph: &Graph) -> u32;
    fn prepare(&mut self, sr: f32, max_block: usize);   // allocate, load IRs
    fn process(&mut self, ctx: &mut BlockContext);      // audio + control
    fn reset(&mut self);                                // clear state (preset change)

    // Persistence
    fn params(&self) -> ParamMap;        // the knobs
    fn set_param(&mut self, id: ParamId, value: f32);
}
```

`process` runs once per block. `BlockContext` hands it:
- a slice of input audio buffers (already time-aligned),
- output buffers to write,
- a control-input map (name → value),
- a control-output map to write into,
- a param-change queue to drain,
- a block index and sample rate.

The node never allocates, never touches the filesystem, never locks. That is
what makes it safe on the audio thread.

### 3.3 Built-in node types

| Category | Nodes |
|---|---|
| **I/O** | Input, Main Out, Send/Return (FX loop), Tuner |
| **Split/Merge** | Split (by frequency), Split (by level), Split (by pitch), Mix, Blend |
| **Dynamics** | Compressor, Limiter, Noise Gate, Envelope Follower |
| **Tone** | EQ (parametric), Tone Stack (amp-style), Cabinet IR |
| **Distortion** | Overdrive, Fuzz, Waveshaper |
| **Modulation** | Chorus, Phaser, Flanger, Vibrato |
| **Pitch** | Pitch Shifter, Harmoniser, Octaver |
| **Time** | Delay, Reverb, Looper |
| **Modelling** | NAM Amp, NAM Pedal, NAM Cab, RTNeural model |
| **Utility** | Gain, Pan, Phase, Invert, DC Block, Meter, Sidechain |
| **Control** | LFO, Envelope Follower, Macro, Math, MIDI Learn |

`Split` is the node that makes your bass idea easy: a crossover feeds two
branches that can be processed completely independently and recombined with a
Mix. Because latency is compensated, the two branches stay in phase.

---

## 4. Compilation: graph → executor

A graph is what the user builds. The **executor** is what runs. They are
separate, and that separation is what keeps the audio thread cheap.

At compile time (never in the callback):

1. **Validate** — all ports connected where required, types match, no
   zero-sample-rate nonsense.
2. **Cycle detection** — feedback loops are allowed but must be explicit.
   Each loop must contain exactly one `Delay`/`Looper`/explicit feedback node,
   which owns the feedback buffer. This is how reverb and feedback work.
3. **Topological sort** — nodes ordered so every node runs after its sources.
4. **Latency pass** — compute each node's latency, then insert compensation
   buffers on every edge that needs it.
5. **Buffer planning** — allocate a fixed pool of block buffers; assign each
   node in-place or scratch slots. No allocation at runtime.
6. **Emit** — a flat list of `(node, buffer bindings)` steps, executed in order.

The executor then just walks the flat list. The graph is never traversed at
runtime — no pointer chasing, no virtual dispatch beyond the node trait, no
allocations.

**Live recompilation.** Editing a preset is allowed while playing. The rule:
never touch the running executor. Build the new graph on a worker thread, then
hand it to the audio thread at a block boundary (a double-buffer swap with a
crossfade if the user wants no click). This is the same pattern used for
preset changes.

---

## 5. Parameter system

Parameters are the knobs. They are the thing users touch most, so they deserve
care.

### 5.1 Identity and range

```
struct ParamSpec {
    id: ParamId,            // stable, persisted
    name: &'static str,     // "Gain", "Mix"
    unit: Unit,             // dB, Hz, ms, %, semitones, cents
    range: Range<f32>,      // min, max
    default: f32,
    curve: Curve,           // Linear, Log, AmpGain, Semitone, msToHz...
    step: Option<f32>,      // for stepped params (preset up/down)
}
```

**Curves matter.** A gain knob that is linear in gain is useless; it should be
linear in dB. A frequency knob should be log. Declaring the curve once, on the
param, means the UI, the MIDI mapping, and the serialiser all do the right
thing automatically.

### 5.2 Smoothing

Every parameter is smoothed before use. A raw gain write clicks; a smoothed one
does not.

- **Per-parameter one-pole**, time constant declared per param (fast for gain,
  slower for filter frequency to avoid zipper noise).
- **Block-boundary application**: the audio thread reads a param value once per
  block, so the one-pole advances in steps of `block_size` samples. Smoothing is
  therefore exact and deterministic.

### 5.3 Modulation

Any parameter can be modulated by any `Control` source, with an amount and a
polarity:

```
Param::Gain  ←  LFO(out) * 0.5
Param::Cutoff ← EnvelopeFollower(out) * 1.0
```

This is how a wah becomes a filter with an expression pedal, and how a tremolo
becomes an LFO on gain. The modulation graph is resolved at compile time into
the same block-boundary read, so it costs nothing at runtime.

### 5.4 Macros and MIDI

- **Macro**: a virtual parameter that drives several real parameters at once.
  "Gain + Presence + Master" is a macro. The UI shows it as one knob.
- **MIDI Learn**: a MIDI CC is bound to a parameter (or macro) with a range.
  Binding is stored in the preset, not hardcoded.

---

## 6. Plugin architecture: how new effects get built fast

This is the part that determines whether the project stays fun. The goal: a new
effect should take an afternoon, not a week.

### 6.1 The authoring loop

```
1. Pick a category → get a template (DSP skeleton + params + UI card)
2. Implement process() — pure function of inputs, params, and internal state
3. Declare params — name, unit, range, curve
4. The UI card is generated from the param spec (knobs auto-built)
5. Test in the host simulator, then on the Pi
```

**The template is the product.** A good template means the author writes DSP,
not boilerplate. Categories: dynamics, filter, distortion, modulation, pitch,
time, modelling, utility, control.

### 6.2 Two tiers

| Tier | Interface | Use for | Trade |
|---|---|---|---|
| **Native** | Implement `Node` directly | Anything that needs the full graph (splitters, mixers, sidechain, feedback) | Fastest, full power |
| **DSP-only** | Implement `DspBlock`: params + `process(in, out)` | 95% of effects | Cannot route audio; simpler, testable on host |

Most effects are `DspBlock`s wrapped in a thin native node. The wrapper handles
latency, params, and the UI. The author writes the maths.

### 6.3 Host simulation

Every `DspBlock` must be testable without a Pi:

- A Rust `no_std` crate (or a C++ header) with the DSP.
- A host binary that reads a WAV, runs the block, writes a WAV.
- A test harness that checks: impulse response, DC behaviour, stability, CPU per
  block, latency.

**Rule: an effect is not done until it runs in the host simulator.** This is
what makes "quickly prototype" real — you hear it on your laptop before you
ever flash the Pi.

### 6.4 Discovery

Nodes self-register:

```
inventory::submit!(NodeRegistration { id: "nam.amp", factory: ... });
```

Adding a file adds an effect. No central switch, no rebuild of a registry. The
UI builds its palette from the registry.

### 6.5 Sandboxing (optional, later)

Third-party effects are a real risk on a device that costs money. Two options,
in order of effort:

1. **Sandbox by convention** — effects are only loadable from a signed
   directory; a bad effect can only glitch audio, never corrupt the preset
   store (it runs in a separate process, fed by shared memory).
2. **WASM effects** — a `DspBlock` compiled to WASM, run in a sandboxed runtime.
   This is the clean answer and is a well-trodden path (Bitwig, REAPER, and
   several plugin hosts do it).

Start with (1). Design the `DspBlock` boundary so (2) is possible later.

---

## 7. Presets, scenes, setlists

```
Setlist
 └─ Preset            (the graph + settings; one song or one sound)
     ├─ Scene         (a saved state of the same graph — different gains,
     │                 different effects on/off, different NAM model)
     ├─ Stomps        (which effects are engaged right now)
     └─ Snapshots     (a full copy, for "before/after" A/B)
```

- A **Preset** is the graph. A **Scene** is a parameter state of that graph.
  Switching scenes keeps the same routing — that is what makes a 10-song set
  playable without rebuilding anything.
- **Stomps** are per-node bypass flags, addressable by MIDI. This is how a foot
  controller drives a big rig with four buttons: map "preset up", "preset
  down", "tuner", "stomp A".
- **Snapshots** let you store a sound you like before experimenting.

**Hot-swap rule.** Switching preset/scene never stops the audio thread. The new
graph is compiled in the background and swapped at a block boundary.

---

## 8. The control surface

| Input | Maps to |
|---|---|
| Expression pedal (resistor) | Any parameter, via a voltage divider into an ADC (or the interface's line input) |
| GPIO footswitches | Stomps, preset/scene up-down, tap tempo, tuner |
| USB MIDI foot controller (the M-VAVE) | Same, plus CC mapping |
| Touchscreen / phone browser | Everything |

**The abstraction is "control bindings", not "MIDI channels".** A binding says
*this physical input controls this parameter*, with a range and a curve. The
same binding table works for a GPIO switch, a MIDI CC, or a web button. That is
what makes the hardware swappable without rewriting the software.

---

## 9. What this means for the software build order

1. **Types + serialisation.** `Node`, `Port`, `Edge`, `ParamSpec`, `Document`.
   JSON round-trip. This is the contract everything else depends on.
2. **The graph executor.** Compile, latency compensation, block buffers, no
   allocation. Test with a hardcoded graph.
3. **A handful of native nodes.** Input, Out, Gain, Mix, EQ, Cab IR. Enough to
   prove the graph.
4. **The `DspBlock` template + host simulator.** Prove that adding an effect
   takes an afternoon.
5. **NAM integration.** The first real modelling node.
6. **The UI.** Start with a web UI over a JSON API; the same API drives the
   touchscreen later.
7. **Control bindings.** GPIO, expression pedal, MIDI.
8. **Product layer.** Preset store, hot-swap, boot, read-only rootfs.

Steps 1–3 are the foundation. If the graph and executor are right, everything
after is additive.

---

## 10. Decisions (settled)

1. **Sample rate: 48 kHz fixed.** NAM models are trained at 48 kHz; other rates
   would need resampling (CPU + latency).
2. **Mono in, stereo out.** Stereo input doubles the cost for little benefit on
   a guitar. Stereo is introduced after the amp (reverb, delay).
3. **Block size: 64 frames** (1.33 ms at 48 kHz), tunable, measured on hardware.
4. **Feedback loops: allowed**, but each loop must contain exactly one
   delay-owning node (reverb/delay/looper) that owns the feedback buffer.
5. **Parameter units: fixed set** — dB, Hz, ms, %, semitones, cents, ratio,
   linear. Adding units later is a migration.
6. **Preset format: JSON** for v1. Binary later if the store grows.
