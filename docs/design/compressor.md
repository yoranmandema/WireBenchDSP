# Compressor block — design

Status: frozen for build. Card: `t_fb8d71e1` (feature).

## 1. What ships

One new block in the dynamics category:

| | |
|---|---|
| id | `dynamics.compressor` |
| name | `Compressor` |
| category | `Dynamics` |
| ports | `in` (audio) → `out` (audio) |

It wraps the existing `pedal_dsp::envelope::EnvelopeFollower` and is a
pure-parameter-mapping block: the envelope follower tracks input level, and
gain is reduced above a threshold by a fixed ratio. No new DSP primitive is
added; this is integration of an existing, well-tested primitive into the
block API.

## 2. Parameters

| id | name | unit | min | max | default | curve |
|---|---|---|---|---|---|---|
| `threshold` | Threshold | dB | -60 | 0 | -20 | AmpGain |
| `ratio` | Ratio | x | 1 | 20 | 4 | Linear |
| `attack` | Attack | ms | 0.1 | 50 | 1 | Log |
| `release` | Release | ms | 1 | 500 | 50 | Log |
| `makeup` | Makeup | dB | 0 | 30 | 0 | AmpGain |
| `mix` | Mix | % | 0 | 100 | 100 | Linear |

`mix` is a dry/wet blend (0 = dry pass-through, 100 = fully compressed).

## 3. Behaviour

- The envelope follower (its own state in the block) tracks `|in|` with the
  configured attack/release time constants. This keeps the compressor
  allocation-free and sample-accurate on the audio thread.
- Level is converted to dB with a floor (`1e-4`, ≈ -80 dB) so silence maps to
  a finite value.
- Gain reduction is `max(0, (level_db - threshold) * (1 - 1 / ratio))`.
  At 4:1, 8 dB over threshold gives 6 dB of reduction; at ratio 1 there is no
  reduction.
- The result is `mix(input, input * db_to_lin(-gr_db + makeup), mix)`.

## 4. Defaults rationale

- Threshold -20 dB: engages on signals a few dB above the noise floor.
- Ratio 4:1: a useful, non-destructive default.
- Attack 1 ms / release 50 ms: fast enough to catch transients, slow enough to
  avoid pumping.
- Mix 100 (fully wet): a compressor is a gain processor, not a parallel
  utility; the UI can add dry when it surfaces this block.

## 5. Tests

`graph/tests/graph.rs` locks the behaviour: spec declaration, ratio-1
bypass, threshold boundary, and steady-state gain reduction on a sustained
tone (~6 dB at -20 dB threshold / 4:1).

## 6. Registration

Appears in the engine's built-in palette via
`graph/src/blocks.rs::builtin_registry()` (added as
`r.add::<Compressor>()`), so the UI can select it without any other change.
