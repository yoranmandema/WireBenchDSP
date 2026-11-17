# Header volume controls (input trim + master output)

Frozen design for `t_c1876846`. Two builder cards implement it: an engine card
(`pedal-builder`) and a UI card (`pedal-ui-builder`), UI behind engine.

## What the existing control actually drives

The card calls the per-stack slider an "input slider". It is not one. In
`ui/src/components/Chain.tsx` the `ChannelHead` range input calls
`setMasterVolume(channel.id, value)`, which the engine handles in
`engine/src/server.rs::set_master_volume` by writing `Channel::master_volume`.
`Rig::to_doc` puts that value in the channel's `util.master` node — the
channel's **output** trim, before the summing node.

So the rig has three gain points and the UI only exposed one of them:

| Gain point | Model | UI today |
|---|---|---|
| pedal input trim | — nothing — | none |
| per-channel output trim | `Channel::master_volume` | the per-stack slider |
| pedal output (limiter) volume | `out__master` node, hard-coded `0.0` | none |

The adjacent "In" checkbox calls `setSources`, which is a real command, but it
is the only control for `Channel::sources` and it is not what the user meant by
a volume control. The card asks for it to go, so it goes; `sources` stays in
the model and the protocol.

## Decisions

### D1 — Two rig-level values, both in `Rig`

```rust
pub struct Rig {
    pub name: String,
    pub channels: Vec<Channel>,
    pub routings: Vec<Routing>,
    /// Pedal input trim, dB. Applied by the `io.input` node, before any channel.
    pub input_gain: f32,
    /// Pedal output volume, dB. Applied by the output limiter node.
    pub master_volume: f32,
}
```

Both default to `0.0` (unity). Ranges follow the block specs they drive:
input gain −60..+24 dB (`util.gain`), master output −60..+12 dB (`util.master`
volume). The engine clamps to those ranges.

`Channel::master_volume` and `setMasterVolume` are **kept**. Presets carry
per-channel trims and the egui debug app still uses the field; the product UI
just stops rendering a control for it. Removing the field would be a migration
the card does not ask for.

### D2 — Input trim is a param on the `io.input` node, not a new node

`graph/src/blocks.rs::IoInput` gains one param:

```rust
ParamSpec { id: "gain", name: "Input gain", unit: Unit::Decibels,
            min: -60.0, max: 24.0, default: 0.0, curve: Curve::AmpGain }
```

The executor already copies the raw input into the `io.input` node's output
buffer (`Executor::run`, `input_slot`). `IoInput::process` now scales that
buffer in place by `db_to_lin(ctx.param(0))`. No new node, no new edge, no
change to graph topology, and `Rig::from_doc`'s backward walk is untouched.

A separate `util.gain` node was rejected: it would have to be inserted between
`in` and every channel head, and `from_doc` would then have to learn to skip it
so it does not come back as a phantom block in a channel.

### D3 — Master output volume is the existing limiter's `volume` param

`Rig::to_doc` already emits `out__master` (`util.master`, volume + ceiling).
Its volume is currently hard-coded `0.0` in three places
(`Rig::out_master`, `server::out_master`, `server::bypass_doc`). All three take
the rig's `master_volume` instead. `ceiling` stays `-1.0`.

### D4 — Persistence rides on the nodes; the preset format does not change

`to_doc` writes `gain` on the `in` node and `volume` on `out__master`;
`from_doc` reads them back with a `0.0` default. Both are optional params, so
every existing preset (which has neither) still loads at unity. No new `Doc`
field, no migration, no change to `docs/IPC-PROTOCOL.md`'s document schema.

### D5 — IPC

Two new commands, both returning the full state like every other mutation:

| Cmd | args | effect |
|---|---|---|
| `setInputGain` | `{value}` | global input trim, dB, clamped −60..24 |
| `setMasterOutVolume` | `{value}` | global output volume, dB, clamped −60..12 |

Names are deliberate: `setMasterVolume` already means the per-channel trim, so
the global one is `setMasterOutVolume`, not a second `setMasterVolume`.

The snapshot gains two keys, `inputGain` and `masterOutVolume`, beside
`source` and `channels`.

Both handlers push `Cmd::SetParam` to the live executor (real-time safe, same
path as `set_master_volume`) and then drop the cached bypass graph
(`*sh.bypass = None`), because that graph is built once from `bypass_doc()` and
would otherwise keep the old trim after the next rebuild.

### D6 — UI: a compact header knob, not the editor knob

`ui/src/components/Knob.tsx` is the editor's knob and is the subject of
`t_d908a668` (knob interaction on master). It is **not** modified. A new
`ui/src/components/HeaderKnob.tsx` provides the header control: a 34 px face
with the same arc/indicator geometry, a label and a `dB` readout beside it,
vertical drag (150 px = full span), double-click to reset, plus
`role="slider"` with arrow-key support.

Placement: input knob is the first child of `<header class="topbar">` (top
left); master knob is the last child of `.top-right` (top right), after the DSP
readout. Both keep the top bar at its 58 px height. Below 820 px the caption
hides and only the face remains.

`ChannelHead` keeps the channel name and loses the range input and the "In"
checkbox. `ui/src/state.ts` drops `setMasterVolume` and `setSources` (nothing
calls them after this) and adds `setInputGain` / `setMasterOutVolume`.

## Consequences the product owner should know

- Per-channel output trims are no longer editable in the UI. The values still
  load from presets and still apply; they are just not shown. If per-channel
  trims are wanted later, they need a new surface (the channel header is now
  just a name).
- Channel input routing (`sources`) has no UI after this. Routing *between*
  channels is the separate cross-channel routing feature (`t_21473ff1` and its
  children), which does not depend on this card.
- The egui debug app is unchanged; it will read and write the new values through
  the same `Rig` but still shows only its old per-channel slider.

## Verification

- Engine: `cargo test --workspace --exclude pedal-shell` from the worktree root
  (after `source scripts/agent-env.sh`). New coverage: a `Doc` round trip
  carries both values, and the `io.input` trim measurably scales the rendered
  output.
- UI: `cd ui && pnpm run build`, then `pnpm test:e2e` (Cypress against the fake
  engine, which must implement the two new commands).
