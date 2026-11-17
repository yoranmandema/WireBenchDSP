# Tremolo block — design

Status: built and verified. Card: `t_b7eef3c6` (feature). Author: pedal-architect.

## 1. What ships

One new block in the Modulation category:

|||
|---|---|
| id | `mod.tremolo` |
| name | `Tremolo` |
| category | `Modulation` |
| ports | `in` (audio) → `out` (audio) |

It is a self-contained block (no new `pedal-dsp` primitive): it wraps the
existing `pedal-dsp::lfo::Lfo` and performs amplitude modulation. The input
gain is swept by an LFO so the volume pulses rhythmically. The gain varies
between `(1 - depth)` and `1`.

## 2. Parameters

| id | name | unit | min | max | default | curve |
|---|---|---|---|---|---|---|
| `rate` | Rate | Hz | 0.1 | 5.0 | 0.6 | Log |
| `depth` | Depth | Linear (0..1) | 0.0 | 1.0 | 0.5 | Linear |
| `mix` | Mix | Percent | 0.0 | 100.0 | 100.0 | Linear |

Mapping into the block:

```text
inner.set_rate_hz(rate)   // applied per block in `process`
gain = 1.0 - depth * (1.0 - lfo01)   // lfo01 in [0, 1] -> gain in [1-depth, 1]
out  = mix(dry, dry * gain, mix/100)
```

Why these bounds:

* **`rate` 0.1–5.0 Hz (log)** covers the slow pulse (0.1 Hz) to a fairly
  fast warble (5 Hz).
* **`depth` 0.0–1.0** is a linear ratio: 0 = no modulation, 1 = full
  sweep. The gain range `(1-depth) .. 1` is chosen so `depth = 0` is an
  exact bypass (see below).
* **`mix` 0–100 % (dry/wet)** matches every other audio block; the tremolo
  defaults to fully wet (100) so it is heard as a rhythmic pulse.

Why bypass is exact at `depth = 0`:

* With `depth = 0`, `gain = 1.0` for every LFO phase, so `wet = input * 1 =
  input` and `mix(dry, wet, a) = dry * (1-a) + input * a = input`. No matter
  the mix, the output equals the input sample-for-sample.

Why the LFO is sample-continuous:

* `pedal_dsp::lfo::Lfo` advances its phase by exactly `rate_hz /
  sample_rate` each call and only ever wraps forward. `process` is called
  once per sample, so the phase is continuous across block boundaries —
  there is no step at the boundary and no click.

Sample-rate handling: `prepare` re-creates the LFO at the block's sample
rate so the phase increment is correct for the host rate. If the host rate
differs from the construction rate, the LFO is re-created here (once per
block, before `process`), so the rate is always in Hz at the true
sample rate.

## 3. Registration

One line in `builtin_registry()` (`graph/src/blocks.rs`):

```rust
r.add::<Tremolo>();
```

At 48 kHz the block compiles to ~160 bytes of stack plus a couple of
registers; it fits any block budget.

## 4. Tests

`graph/tests/tremolo.rs` (run with `cargo test -p dsp-graph --test tremolo`):

1. **registry** — `mod.tremolo` is present with name `Tremolo`, category
   `Modulation`, the three params above, and audio in/out ports.
2. **depth-zero bypass** — `depth = 0` returns the input unchanged
   sample-exact, for dry, wet and mid mix values.
3. **clamp out-of-range** — a rate above the maximum is clamped to the max.
4. **modulation** — a constant-amplitude input comes out with an oscillating
   envelope at the configured rate (peak ≈ 0.5, trough < 0.35 for depth 0.5).
5. **depth and rate control** — a shallower depth swings less; a faster rate
   completes more cycles over the same buffer.
6. **period continuity** — one run of `N*2` samples is byte-identical to two
   consecutive runs of `N` samples (LFO phase is continuous, no click).
7. **reset clears LFO** — after a reset the first sample is a deterministic
   mid-modulation point (0.375 for depth 0.5).

## 5. Out of scope

No other modulation effect, no changes to the shared effect framework or the
registry mechanism, no UI change. Stereo/multi-voice tremolo and a
per-block glyph are left for a future card.

## 6. Design note on `mix`

The tremolo uses a dry/wet `mix` knob, not a gain knob. This keeps the
effect's notion of "depth" decoupled from the global level and matches the
chorus and every other modulation block. To hear the tremolo on its own,
leave `mix` at 100 % (fully wet).
