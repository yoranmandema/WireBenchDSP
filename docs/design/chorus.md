# Chorus block — design

Status: frozen for build. Card: `t_e19aebca` (feature). Children:
`t_c4a56871` (engine block) then `t_7232014a` (UI catalogue test).
Author: pedal-architect.

## 1. What ships

One new block in the modulation category:

| | |
|---|---|
| id | `mod.chorus` |
| name | `Chorus` |
| category | `Modulation` |
| ports | `in` (audio) → `out` (audio) |

It wraps the existing `pedal_dsp::delay::Chorus<2048>` (`dsp/src/delay.rs`,
"Chorus / flanger: a modulated short delay"). That primitive already does
first-order-allpass fractional delay, an LFO, feedback and a dry/wet mix; the
block is a parameter mapping plus integration, not new DSP.

## 2. Parameters

| id | name | unit | min | max | default | curve |
|---|---|---|---|---|---|---|
| `rate` | Rate | Hz | 0.1 | 5.0 | 0.6 | Log |
| `depth` | Depth | ms | 0.5 | 5.0 | 3.0 | Linear |
| `mix` | Mix | % | 0 | 100 | 50 | Linear |
| `feedback` | Feedback | % | 0 | 80 | 0 | Linear |

Mapping into the primitive (`base_ms` is a fixed 8.0 ms, not a knob):

```text
inner.base_ms   = 8.0          // constant
inner.depth_ms  = depth
inner.feedback  = feedback / 100
inner.mix       = mix / 100
inner.set_rate_hz(rate)
```

Why these bounds:

* **Depth ≤ 5 ms** keeps `base_ms − depth ≥ 3 ms`, so the allpass read never
  clamps at its 1-sample floor (that clamp turns the effect into a 1-sample
  comb). 8 ms ± 5 ms spans 3–13 ms, a full chorus-to-flange range.
* **`N = 2048`** is ~42.7 ms at 48 kHz and still ~21 ms at 96 kHz — well past
  the 13 ms worst case, so the buffer never wraps under modulation.
* **Rate 0.1–5 Hz log** covers the slow shimmer (0.1 Hz) to a fast warble.

`depth_ms`/`base_ms`/`feedback`/`mix` are public fields on the primitive;
`set_rate_hz` sets the LFO. Do not add a `Delay` knob: base delay is part of the
block's identity, and `depth` already sweeps it.

## 3. Latency: none declared

`latency_samples()` stays at the default `0`.

The read head is always *behind* the write head, so the block never
anticipates the input — there is no group delay to compensate. Its delay is
time-varying (3–13 ms) and is the sound itself, exactly like `time.delay`
(up to 1 s, also undeclared) and `time.pitchdelay`. Declare nothing and the
executor's latency pass leaves the branch alone. Adding a number here would
make the pass insert a compensating delay line for a delay that moves around
it.

Bypass needs no special handling either: the executor skips `process` on a
bypassed node.

## 4. Registration

One line in `builtin_registry()` (`graph/src/blocks.rs`):

```rust
r.add::<Chorus>();
```

That is the whole "make it available to users" step — the engine serves
`registry_json(&Registry)` over IPC, and the UI builds the rack and the
add-block picker from that snapshot. No protocol change, no preset-schema
change (params ride on the node as they do for every other block).

## 5. Icon

No per-block glyph. `BLOCK_GLYPHS` in `ui/src/components/Icon.tsx` overrides a
category archetype only when the block is *more specific* than its category;
chorus is the archetype of Modulation (the sine-with-a-wobble), so it inherits.
This follows the selection rule recorded in `docs/ICON-SELECTION-ASSESSMENT.md`
(step 3: override only when the role is more specific than the category).

## 6. Known knock-on: the rack catalogue test

`ui/cypress/e2e/rack.cy.js` hard-codes the catalogue:

* `groups the catalogue by category…` asserts exactly
  `["Modelling","Distortion","Tone","Time","Pitch","Utility"]`. Adding a
  `Modulation` block inserts `"Modulation"` between `"Time"` and `"Pitch"`
  (`CATEGORY_ORDER` in `ui/src/components/BlockList.tsx` already lists it).
* `offers every registry block exactly once` asserts `have.length(11)` → 12.

Both expectations move; the UI card owns that file. The e2e suite is not part
of `pnpm run build`, so the engine card can land first without breaking a
check, but the expectation is wrong until the UI card lands.

## 7. Tests

`graph/tests/chorus.rs`:

1. **registry** — `mod.chorus` is present with name `Chorus`, category
   `Modulation`, the four params above, and audio in/out ports.
2. **dry path** — `mix = 0` returns the input unchanged (sample-exact), so the
   wet/dry mix is wired the right way round.
3. **modulation actually happens** — render a sine through
   `io.input → mod.chorus → io.output` at `mix = 100`, estimate the
   input→output lag per window by cross-correlation over lags 0..700 samples,
   and assert the estimated lag *sweeps* (max − min ≥ 100 samples at
   48 kHz, i.e. ~2 ms, with `depth = 5` and `rate = 0.6`). A static delay, a
   plain gain, or a broken LFO all fail this. Also assert finite output and
   peak < 2.0 over the render.
4. **persistence** — a `Doc` JSON carrying the four params round-trips
   (`Doc::from_json` → `to_json` → `from_json`) and compiles and runs finite.

Run: `cargo test --workspace --exclude pedal-shell`.

## 8. Out of scope

Stereo/multi-voice chorus (two detuned taps), vibrato/phaser/flanger blocks, a
per-block icon, and any change to how the registry reaches the UI. The
primitive is mono, like every other block in the graph; the rig is mono per
channel.
