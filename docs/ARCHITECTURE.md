# Pedal DSP — architecture & reference design

A from-scratch, Rust, **native** guitar pedal (macOS / Linux host): a DMA-driven
stereo effect chain, real-time audio I/O, and a rig of gain controls. The DSP
engine (`dsp/`) is platform-agnostic with zero platform dependencies.

```
engine/              pedal-engine — audio process (cpal, egui/eframe)
dsp/                 pedal-dsp — platform-agnostic DSP primitives
graph/               dsp-graph — effect graph + executor
host-sim/            CLI simulator (no platform deps)
shell/               Tauri desktop app: embeds ui/
ui/                  Preact UI (Tauri desktop bundle)
```

---

## 1. Platform decision

| Option | Verdict |
|---|---|
| **Raspberry Pi 5** (chosen) | 2.4 GHz Cortex-A72, 8 GB LPDDR4, USB-C audio + PCIe — ~$60. Native Linux (Debian), headroom for NAM models and a GUI. The settled deployment target. |
| **Mac Mini** (Apple Silicon) | Same app binary as the Pi via `cpal`; the Mac-first UI target. |
| **Linux desktop** (x86_64) | The development and deployment host for the DSP/engine/shell. |

The platform-agnostic crate `dsp/` (pedal-dsp) has zero platform dependencies,
compiling identically on macOS, Linux and wherever it is used in a thin shell.
Platform-specific audio I/O lives in the thin `engine/` and `live/` shells.

### Why no external codec

On the Pi 5 / Mac / Linux desktop there is no need for an external I2S codec:
`cpal` provides a unified audio API over the platform's native audio device
(CoreAudio, ALSA/PulseAudio, PipeWire). Audio calibration is done in software
instead of via SAI/MCLK registers.

---

## 2. Signal chain

```
 mic / line-in ──► [input trim] ──► [DSP chain] ──► master volume ──► speakers / headphones
                      │                        │
                      ▼                        ▼
                 (In)                    (cpal / ALSA / PipeWire)
```

### Digital chain

```
in ─► DC block ─► distortion (4× oversampled) ─► tone stack ─► delay ─► reverb ─► DC block ─► out
```

* **DC block** — 20 Hz high-pass, removes offset so the digital stages never see
  a DC component.
* **Distortion** — tanh / soft-clip / hard-clip / tube / fuzz / rectify, run
  through a 4× oversampling stage (two cascaded half-band FIRs) to push aliasing
  out of the audio band.
* **Tone stack** — a 200 Hz low shelf and a 4 kHz high shelf (RBJ biquads),
  ±12 dB.
* **Delay** — stereo feedback delay, up to 100 ms, with fractional reads.
* **Reverb** — Freeverb (8 combs + 4 allpasses).

---

## 3. Hardware: Raspberry Pi 5

The Pi 5 is the settled deployment target. It runs a native Linux (Debian /
Raspberry Pi OS) and carries USB-C audio plus PCIe, so the whole stack (engine,
DSP, UI) runs as a native Linux application.

| Component | Chip | Notes |
|---|---|---|
| Main CPU | Broadcom Cortex-A72 | 2.4 GHz, 8 GB LPDDR4 |
| Audio | USB-C (cpal) | Native audio, no external codec |
| Model storage | eMMC + SD | NAM models loaded at runtime |
## 7. Build & flash

```bash
# Build the native audio engine + host app
cargo build --release

# Run the engine (headless, null audio device) and verify
cargo run --example latency_probe --release
cargo test
```

See `docs/BUILD-AND-FLASH.md` for the full toolchain and debugging setup.

---

## 8. Testing

```bash
cd dsp
cargo test          # 49 tests: biquads, FIR, oversampling, waveshaping,
                    # delay, envelopes, LFO, reverb, DC blocker, SVF
```

The tests run on the host, so DSP algorithms can be validated without
hardware. `pedal-dsp` is `no_std` and allocator-free (const-generic buffers).
