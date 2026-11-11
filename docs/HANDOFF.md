# HANDOFF — pedal-dsp

**Written:** 2026-11-11
**For:** continuing this project in a fresh session (possibly on a local model).
**Read this first, then `PRODUCT-MODEL.md`, `EXECUTOR.md`, `DSP-BLOCKS-AND-SIMULATOR.md`.**

---

## 1. What this is

A from-scratch, Rust, **modelling guitar pedal**. The product idea is a
Quad-Cortex-like box: a free-form signal graph (amp/cab/effects/modulation),
editable on a screen, driven by a foot controller and an expression pedal.

The user is a **software-comfortable, electronics-beginner "product owner"**.
They want to *vibe-code* this, so the agent should explain electronics and
hardware choices in plain language and not assume EE knowledge. They want to
make the musical/UX decisions; the agent does the engineering.

### The one idea that defines the software

**A preset is a graph.** Nodes have typed audio ports; edges connect them; a
document is nodes + edges + per-node params, serialised as JSON. Parallel
branches, splitters, side-chains, "compressor after the amp but before the
cab", "pitch shifter after everything" — all of that is just graph topology.
This is the user's explicit requirement and it is already designed and partly
built.

---

## 2. Hardware status — **decision not finalised**

The user was leaning **Raspberry Pi 5** and already owns:
- a **M-VAVE "Chocolate"** Bluetooth MIDI foot controller (4 buttons)
- a **cheap expression pedal**
- some **I2S hardware**

### Research findings (from the earlier session)

| Topic | Finding |
|---|---|
| Quad Cortex engine | Analog Devices **SHARC ADSP-2156x** DSP, not a general-purpose CPU. Proves a modelling pedal does not need a Linux application processor. |
| Pi 5 as a pedal | Workable. **PiPedal** already turns a Pi into a NAM pedal with a web UI. But the user wants to write their own software, so the "just install PiPedal" route is out — PiPedal is inspiration, not a dependency. |
| Latency | The real risk on Linux. NAM's own guidance is 32×4 audio buffers; that is ~2.7 ms at 48 kHz, before the model itself. Needs real measurement on the Pi 5. |
| Open-source references | **PiPedal**, **Pi-MFX**, **OpenRig**, **coyopedal** — all permissively licensed; read them for architecture, don't vendor them. |
| Bluetooth MIDI | **Will not work** on a Pi 5 for this use: the built-in BT shares the USB bus with Ethernet and adds jitter. Use the Chocolate over **USB-C wired** (it enumerates as a standard USB-MIDI device). |
| STM32 path | Still viable and cheaper. `docs/ARCHITECTURE.md` has the full H723 + SGTL5000 design. The DSP library is portable, so nothing is lost by deciding late. |

### The architectural consequence (already applied)

**The DSP and graph crates have zero platform dependencies.** No ALSA, no
CoreAudio, no filesystem, no `std::process`. They compile identically for
macOS (dev), Linux/Pi 5 (deployment) and bare-metal STM32 (alternative
deployment). The platform-specific code lives in thin shells:

| Crate | Platform deps | Mac | Pi 5 | STM32 |
|---|---|---|---|---|
| `dsp` (pedal-dsp) | none | ✅ | ✅ | ✅ |
| `graph` (dsp-graph) | none | ✅ | ✅ | ✅ |
| `host-sim` | none (CLI) | ✅ | ✅ | — |
| `live` (pedal-live) | cpal, egui, eframe | ✅ dev rig | ✅ | — |
| `firmware` | stm32h7xx-hal, defmt | — | — | ✅ |

**This is why the hardware decision can wait.** Nothing downstream cares.

### Settled decisions (from `PRODUCT-MODEL.md` §10)

1. **Sample rate: 48 kHz fixed.** NAM models are trained at 48 kHz; other rates
   need resampling (CPU + latency).
2. **Mono in, stereo out.** Stereo is introduced *after* the amp (reverb/delay).
3. **Block size: 64 frames** = 1.33 ms at 48 kHz. Tunable; measure on hardware.
4. **Feedback loops allowed**, but each loop must contain exactly one
   delay-owning node that owns the feedback buffer.
5. **Parameter units: fixed set** — dB, Hz, ms, %, semitones, cents, ratio, linear.
6. **Preset format: JSON** for v1.

---

## 3. Repo layout

```
~/Documents/Repos/pedal-dsp/          (git repo)

Cargo.toml                            workspace: dsp, graph, host-sim
                                      (firmware/ and live/ are EXCLUDED — separate workspaces)

dsp/                                  pedal-dsp — no_std DSP library
  src/biquad.rs                       RBJ biquads + shelving/peak/notch
  src/svf.rs                          state-variable filter
  src/delay.rs                        fractional delay, FeedbackDelay, PitchShifter
  src/pitch_vocoder.rs                NEW — phase vocoder pitch shifter
  src/pitch_remap.rs                  NEW — bin-remap pitch shifter
  src/bin_shift.rs                    NEW — simple bin shift
  src/fft.rs                          NEW — small FFT for the vocoder
  src/fir.rs                          windowed-sinc FIR, half-band decimator
  src/oversample.rs                   2x/4x oversampling + half-band
  src/waveshape.rs                    tanh/atan/soft-clip, Chebyshev drive
  src/envelope.rs                     ADSR + envelope follower
  src/lfo.rs                          sine/saw/square/triangle LFO
  src/reverb.rs                       feedback comb/allpass reverb
  src/dc_blocker.rs                   DC removal
  src/chain.rs                        helper chain builder
  src/util.rs                         db_to_lin, clamp, etc.
  examples/                           pitch_probe, vocoder_probe, wav_process, alias_check

graph/                                dsp-graph — node/edge graph + executor
  src/types.rs                        BlockSpec, PortSpec, ParamSpec, Unit, Curve
  src/block.rs                        Block trait, BlockCtx, BlockError
  src/blocks.rs                       Gain, ToneStack, Overdrive, Reverb, Delay,
                                      IoInput, IoOutput, MixSum
  src/graph.rs                        Graph: nodes, edges, validate, topo_sort
  src/executor.rs                     Executor: compile → step list + buffer pool
  src/registry.rs                     Registry: id → BlockSpec, builds blocks
  tests/graph.rs                      11 integration tests

host-sim/                             CLI simulator (no platform deps)
  src/main.rs                         rig builder, bench, palette, impulse, WAV
  src/blocks.rs                       re-exports dsp-graph blocks

firmware/                             STM32H723 firmware (separate workspace)
  src/main.rs, audio.rs, board.rs, codec.rs, controls.rs

live/                                 pedal-live — Mac/Pi real-time rig runner
  src/main.rs                         cpal audio I/O + egui/eframe UI

docs/
  ARCHITECTURE.md                     original STM32 H723 + SGTL5000 design
  ANALOG-FRONTEND.md                  analogue front-end design
  BUILD-AND-FLASH.md                  firmware build/flash
  PRODUCT-MODEL.md                    ← the product spec (read this)
  EXECUTOR.md                         ← executor design (read this)
  DSP-BLOCKS-AND-SIMULATOR.md         ← block + simulator design (read this)
  HANDOFF.md                          ← this file
```

---

## 4. Where it stands — **all tests green**

```
cargo test --workspace     →  11 passed (dsp-graph)
                              62 passed (pedal-dsp)
                               2 passed, 1 ignored (examples)
                              0 failed
```

### What is built and verified

**`dsp` crate** — a real, tested, no_std DSP library. The pitch work is the
most recent and most interesting:

- **`PitchShifter`** (delay-line based) — measured pitch accuracy within
  **±0.4 cents** over ±2400 cents at 48 kHz. Has three modes: instant jump,
  **glide/portamento**, and a **continuous sweep rate (cents/sec)** that never
  settles — the user's specific requirement for a Rainbow-Machine-style effect.
  Shepard mode (endless rise, bounded spectrum) is implemented.
- **`PitchVocoder`** (phase vocoder) — implemented this session, and **a real
  bug was found and fixed** (see §5). Now accurate to **±1.6 cents**.
- Plus biquads, SVF, FIR, oversampling, waveshaping, envelope, LFO, reverb,
  DC blocker, FFT, bin-shift and pitch-remap experiment.

**`graph` crate** — the free-form graph the user asked for:

- `Block` trait with typed audio ports, params, and `prepare/process/reset`.
- `Graph` with `add`/`connect`/`remove`/`set_param_real`, validation
  (missing inputs, cycles), and Kahn topological sort.
- `Executor` — compiles the graph into a flat `Vec<Step>` over a pre-allocated
  buffer pool. The audio callback walks the step list; **zero allocation** in
  the audio path. The one `unsafe` boundary is the buffer-pool views, whose
  invariant (a step's input and output slots are always distinct) is enforced
  by the planner.
- `Registry` — id → spec, builds blocks by id. Drives the UI palette.
- Params are stored **normalised (0..1)** in the graph and **denormalised to
  real units** (dB/Hz/ms/%) by the executor before reaching the block. Blocks
  never see a curve. This was a real bug (see §5).
- `IoInput`/`IoOutput` are the graph boundary; `MixSum` is the recombine node.

**`host-sim`** — runs on the Mac with no hardware:

```
cargo run --release -p host-sim -- --rig demo    --bench
cargo run --release -p host-sim -- --rig parallel --bench
cargo run --release -p host-sim -- --palette
cargo run --release -p host-sim -- --rig demo --impulse
cargo run --release -p host-sim -- --in input.wav --out output.wav --rig demo
```

`host-sim` flags: `--rig <demo|parallel>`, `--bench`, `--impulse`, `--palette`,
`--in <wav>`, `--out <wav>`.

Two rigs are built in:
- **`demo`** — in → drive → tone → (dry + delay + reverb) → out
- **`parallel`** — in → (clean ∥ drive→tone) → mix → out, the bass
  channel-split patch the user asked for

Measured on the Mac (M-series), 64-frame blocks, 144 000 frames:

| rig | peak | rms | clipped | non-finite | per-block | budget used |
|---|---|---|---|---|---|---|
| demo | 1.72 | 0.227 | 1573 | 0 | 1631 ns | 0.1% |
| parallel | 1.69 | 0.235 | 3170 | 0 | 804 ns | 0.1% |

**Both rigs clip** — the drive + reverb + delay sum exceeds full scale. This is
a gain-staging gap, not a bug: there is no master volume/limiter block yet.
**That is the first thing to fix** (see §6).

---

## 5. What happened in the last session (the two bugs)

### Bug 1 — parameters were normalised but blocks read them as real

The graph stored params normalised (0..1); the executor passed them straight
through. So `set_param_real("drive", "drive", 4.0)` reached the block as `0.4`,
and `set_param_real("mix", "a", 0.7)` reached it as `0.00007`. The parallel rig
was essentially silent (peak 0.0001).

**Fix:** the executor now denormalises at compile time and on every
`set_param_real`. `Executor::params_real` holds the real values; `BlockCtx`
gets those. Added a regression test (`params_are_denormalised_before_reaching_the_block`).

### Bug 2 — the phase vocoder shifted to the wrong pitch

`pitch_vocoder.rs` measured the phase deviation between analysis frames but
computed the expected advance with the **synthesis** hop `hs = hop_a * ratio`
instead of the **analysis** hop `hop_a`. The wrapping then produced a bin
frequency that was wrong by a ratio-dependent factor.

Symptom: 220 Hz → 407 Hz on an octave up (should be 440), 440 Hz → 252 Hz on
an octave down (should be 220). Exactly the 133/239-cent errors the tests
reported.

**Fix:** two characters — `hs` → `ha` in the deviation and the division. Both
octave tests now pass within 10 cents; the probe shows ±1.6 cents.

**Lesson for the next session:** the host simulator and the probe examples are
what caught both bugs. Keep using them.

---

## 6. Known issues and next steps (in priority order)

### P0 — the graph is not yet a full product

1. **Master volume / limiter.** Both rigs clip. Add a `Master` block (gain +
   soft limiter) and put it before `IoOutput`. Re-run the rigs; clipping should
   drop to 0. *This is the most visible remaining defect.*
2. **Latency compensation is designed but not implemented.** `EXECUTOR.md` §2.4
   specifies the pass: each node reports `latency_samples()`; the executor
   inserts delay lines on faster branches so parallel paths recombine in phase.
   The `Block` trait has no `latency_samples()` yet. **Without this, any
   parallel branch with different latency will comb-filter.** This matters
   immediately because the user's headline feature is parallel branches.
3. **Cycle handling is "reject" only.** `validate()` returns `Cycle`. The design
   says a loop must contain exactly one delay-owning node; implement that
   instead of rejecting all cycles.
4. **Stereo.** Everything is mono. The plan is mono until after the amp, then
   stereo for reverb/delay. `PortSpec` needs a channel count; the buffer plan
   needs to allocate 2 slots per stereo buffer.

### P1 — the product layer

5. **Serialisation.** JSON round-trip for a whole document. No serde in the
   platform-free crates yet — decide whether to add `serde` (it is `no_std`
   compatible with `alloc`) or hand-roll. `DSP-BLOCKS-AND-SIMULATOR.md` has a
   sketch of the format.
6. **NAM integration.** The first real modelling node. NAM models are `.nam`
   files; the loader and the inference loop are the work. This is the step that
   makes it a *modelling* pedal rather than an effects box.
7. **Cab IR.** Convolution with a wav IR; `fir.rs` and `fft.rs` already exist.
8. **The UI.** Design says: web UI over a JSON API first, touchscreen later.
   The same API drives both. The `live/` crate (cpal + egui) is the desktop
   stand-in and is the fastest way to get knobs on screen.

### P2 — hardware

9. **Measure latency on the Pi 5.** Until this is done the hardware decision is
   not actually settled. Target: < 10 ms round-trip, ideally < 5 ms.
10. **Decide Pi 5 vs STM32 H723.** The software is ready for either. The
    decision should be driven by the latency measurement and by whether the
    user wants a screen (Pi) or a purpose-built box (STM32).
11. **Wiring/bench.** `docs/ANALOG-FRONTEND.md` and `docs/BUILD-AND-FLASH.md`
    have the H723 design. The Pi path needs a different analogue front-end
    (I2S ADC + DAC, e.g. PCM3060 or a combined codec) — not yet designed.

### P3 — polish

12. **Preset store, hot-swap, boot, read-only rootfs** (Pi path).
13. **Control bindings.** The abstraction is "control bindings", not MIDI
    channels: a binding maps a physical input to a parameter with a range and
    curve. Same table drives GPIO, expression pedal, MIDI, or a web button.
    The M-VAVE Chocolate should be used **wired USB-MIDI**, not Bluetooth.

---

## 7. Conventions the next session must know

- **Params in the graph are normalised (0..1).** `set_param_real` denormalises;
  `set_param_norm` sets raw. Blocks always receive real values. Never pass a
  normalised value to a block.
- **`Unit::Percent` real values are 0–100**, and the block divides by 100.
  `Unit::Ratio` real values are 0–1. This tripped us once already.
- **`cargo test --workspace`** does **not** build `firmware/` or `live/` — they
  are separate workspaces with their own `target/` dirs.
- **`graph/` and `host-sim/` are uncommitted** (`git status` shows them as
  `??`). The `dsp/` changes are partly uncommitted too. **Commit before
  switching sessions** if you want the work safe.
- The repo has a `.gitignore`d `target/`; `output.wav` is a generated artifact.
- No passwordless sudo on this Mac — privileged commands must be handed to the
  user.

---

## 8. Commands cheat-sheet

```bash
cd ~/Documents/Repos/pedal-dsp

# everything
cargo test --workspace
cargo build --workspace --release

# the simulator
cargo run --release -p host-sim -- --rig demo     --bench
cargo run --release -p host-sim -- --rig parallel --bench
cargo run --release -p host-sim -- --palette
cargo run --release -p host-sim -- --rig demo --impulse
cargo run --release -p host-sim -- --in input.wav --out output.wav --rig demo

# DSP probes
cargo run --release -p pedal-dsp --example pitch_probe
cargo run --release -p pedal-dsp --example vocoder_probe
cargo run --release -p pedal-dsp --example wav_process -- input.wav output.wav [preset]

# firmware (separate workspace)
cd firmware && cargo build --release

# desktop live rig (separate workspace)
cd live && cargo run --release
```

---

## 9. Open questions for the user

1. **Pi 5 or STM32?** Still open. The latency measurement (P2.9) should decide it.
2. **Screen?** A Pi with a touchscreen is the fastest route to a Quad-Cortex
  -like UI. A STM32 box needs a separate display module and a UI protocol.
3. **How much of the analogue front-end does the user want to build?** They are
   an electronics beginner and asked for hand-holding. The Pi path needs an
   I2S ADC/DAC board and a instrument-level input buffer; that is a
   well-trodden but real electronics project.
4. **NAM licensing.** NAM models are community-trained; confirm the user is
   comfortable with that ecosystem before building the loader.
