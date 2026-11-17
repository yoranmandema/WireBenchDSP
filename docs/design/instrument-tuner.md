# Instrument tuner: detection, plumbing, and UI contract

Status: **decided** (architect card `t_b366a8fb`). This document freezes the
detection algorithm, the reading model, the threading, and the wire contract so
the engine and UI cards can be built without re-litigating them.

Related: `docs/IPC-PROTOCOL.md` (wire format), `docs/UI-ARCHITECTURE.md`,
`engine/src/audio.rs`, `engine/src/host.rs`, `engine/src/server.rs`.

## 1. What it is

A chromatic tuner for the **instrument input**, opened from a button in the top
bar and shown in a modal. It answers one question: *what note am I playing, and
how far off is it?*

Three decisions that shape everything below:

1. **The engine detects the pitch, not the UI.** The UI never sees audio — it
   only receives meters and state. The input samples exist in exactly one place:
   the engine's input callback. So the detector lives in the engine.
2. **The tuner listens to the physical input, not to the selected source.** The
   `RIFF` / `LIVE` / `SILENCE` selector chooses what the *rig* processes. A
   musician tunes while the riff is playing, so the tuner taps the input stream
   itself and is unaffected by that selector.
3. **The tuner is always analysing; there is no on/off command.** A 2048-sample
   pitch estimate costs ~1 Mflop and runs ~23 times a second — under 0.1% of a
   core. Gating it behind a `setTuner` command would save nothing measurable and
   would add a state the UI can get wrong (open the modal, forget to enable, see
   a dead panel). No command, no flag, no extra protocol surface.

## 2. Detection algorithm (frozen)

**YIN** (de Cheveigné–Kawahara), the standard monophonic pitch estimator. It is
chosen over an FFT peak pick because a tuner needs *cents* accuracy: a 2048-point
FFT at 48 kHz has 23 Hz bins, and parabolic interpolation on a plucked-string
spectrum is not accurate enough. Autocorrelation gives sub-sample period
accuracy directly.

`engine/src/tuner.rs`:

```rust
pub const REFERENCE_HZ: f32 = 440.0;   // A4
const WINDOW: usize = 2048;            // 42.7 ms at 48 kHz
const THRESHOLD: f32 = 0.15;           // YIN absolute threshold
const MIN_CLARITY: f32 = 0.5;          // below this, report "no note"
const MIN_RMS: f32 = 0.002;            // below this, the input is silent
```

Range: `tau_min = floor(sr / 1400)` (34 at 48 kHz → up to ~1400 Hz) and
`tau_max = ceil(sr / 70)` (686 → down to ~70 Hz). That covers E2 (82.4 Hz)
through E6 (1318.5 Hz) with margin on both sides.

Steps, exactly:

1. **Gate on level.** `rms = sqrt(mean(x²))` over the window. Below `MIN_RMS`,
   return `None`. This is what makes silence read as "no note" rather than as
   noise.
2. **Difference function.** For `tau` in `1..=tau_max`:
   `d[tau] = Σ_{j=0}^{WINDOW-tau-1} (x[j] − x[j+tau])²`.
   A DC offset cancels in the subtraction, so no high-pass is needed.
3. **Cumulative mean normalisation.**
   `d'[tau] = d[tau] · tau / Σ_{j=1}^{tau} d[j]`, with `d'[0] = 1`.
4. **First dip.** The smallest `tau ≥ tau_min` with `d'[tau] < THRESHOLD`, then
   walk forward while `d'[tau+1] < d'[tau]` to the bottom of that dip. (Not the
   global minimum: the first dip is the fundamental; later dips are multiples.)
5. **Clarity.** `clarity = 1 − d'[tau]`. Below `MIN_CLARITY`, return `None`.
6. **Parabolic interpolation** on `d'` around `tau` (only when `tau < tau_max`):
   `shift = (d'[tau−1] − d'[tau+1]) / (2 · (d'[tau−1] − 2d'[tau] + d'[tau+1]))`,
   clamped to ±1. `period = tau + shift`, `freq = sr / period`.
7. Convert to a note (§3).

A `None` result is a **valid answer** ("no reliable note"), not an error. The
UI shows a "play a note" prompt for it.

### 2.1 Streaming wrapper

```rust
pub struct Detector { /* … */ }
impl Detector {
    pub fn new(sample_rate: f32) -> Self;
    pub fn push(&mut self, sample: f32);      // accumulate
    pub fn ready(&self) -> bool;              // buffer >= WINDOW
    pub fn detect(&mut self) -> Option<TunerReading>;  // analyse, then drain WINDOW
}
```

`detect` consumes a whole window (`HOP = WINDOW`), so the estimate refreshes
every 42.7 ms at 48 kHz. Overlapping windows are not worth the extra CPU for a
needle the UI redraws at 20 Hz.

### 2.2 Known limitations (accepted, not fixed here)

* **Symmetric waveforms.** A perfectly symmetric wave (square, triangle, an
  octave-doubled tone) has a difference function that also dips at half the
  period, so YIN can lock an octave high. A plucked guitar string is not
  symmetric; the first-dip rule is the correct trade for noise robustness.
* **Polyphony.** One note at a time. Two notes at once give a reading somewhere
  between them; the clarity usually drops and the UI falls back to "no note".
* **Very low input.** Below `MIN_RMS` there is no reading. The gate is low on
  purpose so a quietly played guitar still tunes.

## 3. Reading model

```rust
pub struct TunerReading {
    pub active: bool,      // false = no reliable note
    pub freq: f32,         // Hz, 0.0 when inactive
    pub note: &'static str,// "C"…"B", "" when inactive
    pub octave: i32,       // 0 when inactive
    pub cents: f32,        // −50…+50, 0.0 when inactive
    pub clarity: f32,      // 0…1, 0.0 when inactive
}
```

`impl Default` is the inactive reading, so a fresh engine reports "no note"
rather than a fabricated one.

Note naming: `midi = 69 + 12·log2(freq / REFERENCE_HZ)`, `note_index = round(midi)`,
`cents = (midi − note_index)·100` clamped to ±50, name from
`["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"]` by
`note_index.rem_euclid(12)`, octave `note_index.div_euclid(12) − 1`.

`A4 = 440 Hz` is a constant. There is deliberately **no reference control** in
this feature; the UI shows the reference as text so the number is not a mystery.

## 4. Plumbing and threading

```
input callback ──push──► tuner ring (SPSC) ──pop──► tuner thread ──► Meters.tuner
 (real-time, +1 store)      8192 samples         (normal priority)      │
                                                                        ▼
                                            server tuner_loop ──event: tuner──► UI
```

* **The input callback** (`host::build_input`) already reduces each frame to one
  mono sample `s`. It pushes `s` into the audio ring as today **and** into a
  second `Ring` for the tuner. One extra store per sample; nothing else in the
  callback changes. `headless::start` does the same with its generated tone.
* **The tuner ring** is created by `host::start` / `headless::start` (capacity
  `RING_CAPACITY`). `Ring` stays strictly SPSC: producer = input callback,
  consumer = the tuner thread. The output callback never touches it.
* **The tuner thread** is a plain `std::thread` spawned once by `main.rs`:
  drain whatever is available (up to 2048 samples) into a `Detector`, and
  whenever `detect()` returns a reading, publish it. It sleeps 20 ms between
  drains. It is never on the audio path and never allocates after start-up.
* **The reading** lives in `Meters` as `pub tuner: Mutex<TunerReading>` — the
  same shared, `Arc`-wrapped struct that already carries the peaks and CPU time
  and is already handed to `host::start`, `headless::start` and `server`.
  `Meters` is `Default`, so `new_engine` and every test that builds a `Handle`
  are unchanged.
* **The server** broadcasts `event: tuner` at 50 ms (20 Hz) to every connected
  client while any client is connected, exactly like `meters`. No snapshot
  change: the reading is not part of `state`, because it changes far faster than
  the state does and because a reconnecting UI must not have to wait for a
  mutation to see it.

Nothing in this path touches the executor, the command queue, or the compiled
graph. The UI remains off the audio path.

## 5. IPC contract

One new unsolicited event. No new command, no change to `state`.

```jsonc
{ "type": "event", "event": "tuner",
  "payload": {
    "active": true,
    "freq": 220.01,      // Hz
    "note": "A",
    "octave": 3,
    "cents": 0.1,        // −50…+50
    "clarity": 0.94,     // 0…1
    "reference": 440.0   // the fixed A4 the note name is derived from
  } }
```

Inactive reading: `active: false`, `freq: 0.0`, `note: ""`, `octave: 0`,
`cents: 0.0`, `clarity: 0.0`, `reference` unchanged.

20 Hz, best-effort, no `rev` semantics (it is not state). A client that
disconnects and reconnects simply starts receiving it again.

## 6. UI contract

`ui/src/protocol.ts`:

```ts
export interface Tuner {
  active: boolean;
  freq: number;
  note: string;
  octave: number;
  cents: number;   // −50…+50
  clarity: number; // 0…1
  reference: number;
}
export interface TunerEvent { type: "event"; event: "tuner"; payload: Tuner }
export function isTuner(msg: ServerMessage): msg is TunerEvent;
```

`ui/src/state.ts` gains `export const tuner = signal<Tuner>({ …inactive… })`,
`export const tunerOpen = signal(false)`, and folds the event in `init()`
beside `isState` / `isMeters`. There is no action to send.

The modal is a **read-only view**; it sends nothing to the engine. Closing it
changes no engine state.

## 7. Non-goals

* No reference-pitch control (A4 ≠ 440), no temperament, no sweetened tuning.
* No per-string preset (guitar/bass/ukulele), no auto-detect instrument.
* No tuner *block* in the signal chain, and no gate/mute driven by the tuner.
  `PRODUCT-MODEL.md` lists "a gate from a tuner can mute the output" as a future
  control-routing feature; it is not part of this card.
* No history, no cents-over-time graph, no microphone input other than the
  interface the engine already uses.
* No change to the `RIFF` / `LIVE` / `SILENCE` selector.
