# Cross-channel routing: data model, persistence, and contracts

Status: **decided** (architect card `t_21473ff1`). This document freezes the
model and the signal semantics so the engine and UI cards can be built without
re-litigating them.

Related: `docs/IPC-PROTOCOL.md` (wire format), `docs/ARCHITECTURE.md`,
`engine/src/channels.rs`, `graph/src/doc.rs`.

## 1. What a routing is

A **routing** is a *tap*: it takes one block's output and **additionally** feeds
a block in another channel. It is not an edge and not a channel source.

| Concept | Where it lives | Meaning |
|---|---|---|
| Block wire | `Doc.edges` | the single downstream connection of a block's output port |
| Channel source | `Channel.sources` | which channel master (or the pedal input) feeds a channel's head |
| **Routing** | `Rig.routings` / `Doc.routings` | an extra feed from one block to another block in a different channel |

The source block keeps its own downstream connection untouched. Creating a
routing must not change `Doc.edges`, `Doc.nodes`, or `Channel.sources`; the
compiled graph before the routing feature is byte-identical.

A routing is **not** an `EdgeDoc`:

* an edge *replaces* the wire into an input port (`Graph::validate` rejects a
  second wire into the same port — `GraphError::MultipleInputs`);
* a routing *adds* a feed, so it needs a summing node at the destination.

## 2. Data model

`graph/src/doc.rs`:

```rust
pub struct RoutingEndpoint { pub channel: String, pub block: String }

pub struct Routing {
    pub id: String,                    // stable, used by removeRouting / setEnabled
    pub source: RoutingEndpoint,
    pub dest: RoutingEndpoint,
    #[serde(default = "default_enabled")] pub enabled: bool,   // default true
    #[serde(default = "default_gain")]   pub gain: f32,        // linear, default 1.0
}
```

`Doc` carries `routings: Vec<Routing>` beside `nodes` and `edges`
(`#[serde(default, skip_serializing_if = "Vec::is_empty")]`).

`engine/src/channels.rs` — `Rig`:

```rust
pub struct Rig {
    pub name: String,
    pub channels: Vec<Channel>,   // always exactly CHANNEL_COUNT (4) slots
    pub routings: Vec<Routing>,  // creation order; separate from channels/sources/edges
}
```

`RoutingEndpoint` is `{channel, block}` — both ends are *block* instances, not
channel masters and not the pedal input. A routing cannot start at `In` or end
at `Out`; those are channel-source concerns.

### JSON

```jsonc
{
  "name": "Crunch",
  "nodes": [ /* … */ ],
  "edges": [ /* … */ ],
  "routings": [
    { "id": "route1",
      "source": { "channel": "ch1", "block": "amp" },
      "dest":   { "channel": "ch2", "block": "delay" },
      "enabled": true,
      "gain": 1.0 }
  ]
}
```

`routings` is omitted when empty. `enabled` and `gain` are optional on read and
default to `true` / `1.0`, so a hand-written or pre-feature project still loads.

## 3. Rust API

`Rig` (all in `engine/src/channels.rs`):

| Method | Behaviour |
|---|---|
| `create_routing(&mut self, source, dest) -> Result<String, String>` | validates, pushes with `enabled: true`, `gain: 1.0`, returns a fresh id (`route1`, `route2`, …) |
| `remove_routing(&mut self, id) -> bool` | removes by id; `false` when absent |
| `set_enabled(&mut self, id, enabled) -> Result<(), String>` | unknown id is an error; re-enabling a routing that would close a cycle is refused |
| `set_gain(&mut self, id, gain) -> Result<(), String>` | linear gain |
| `list_routings(&self) -> &[Routing]` | creation order |
| `routing(&self, id) -> Option<&Routing>` | one routing |
| `validate_routing(&self, source, dest) -> Result<(), String>` | the rules below |
| `routing_creates_cycle(&self, source, dest) -> bool` | reachability over the block-level signal graph |

## 4. Validation rules (frozen)

`createRouting` refuses, with `ok: false` and a message, and the routing never
enters the list:

1. unknown channel, or unknown block in the channel;
2. `source.block == dest.block` (a block routing to itself);
3. `source.channel == dest.channel` — a routing must cross channels;
4. an exact duplicate (`source` **and** `dest` already present);
5. the new edge would close a feedback loop.

Multiple routings may share a source (fan-out) and may share a destination
(summed). Both are allowed.

Cycle detection is over the **block-level signal graph**: each channel's chain
(block order + its `util.master`), each channel's source feeds into its head,
and the *enabled* routings. A candidate routing `s → d` is a cycle when `d`
can already reach `s`. `Doc::would_loop` is the edge-level analogue and is not
used for routings.

## 5. Lifecycle

* **Create** — validated, appended, `enabled: true`, `gain: 1.0`.
* **Toggle** — kept in the list and in the saved project; a disabled routing is
  simply absent from the signal path. Re-enabling re-runs the cycle check.
* **Remove** — gone from the list, from the audio path, and from the next save.
* **Reorder a block** (`moveBlock`) — routings reference block **ids**, not
  indices, so reordering never disturbs them.
* **Delete a block** (`removeBlock`) — every routing touching that block as
  source **or** destination is removed with it. A routing whose endpoint no
  longer exists cannot be honoured, and leaving it would draw a line to a block
  that is not there. (Implemented in `Rig::remove_block`.)
* **Delete a channel** — not possible: a rig always has exactly `CHANNEL_COUNT`
  slots. Emptying a channel removes its blocks, which prunes their routings.
* **Load** — `Rig::from_doc` validates every routing in the file and **drops**
  the invalid ones (logging to stderr), so a hand-edited project cannot smuggle
  in a self-route, a same-channel route, a duplicate or a cycle. Valid ones are
  kept in file order.

## 6. Persistence

The project file **is** the preset `Doc` (`presets/*.json`). `Rig::to_doc()`
copies `routings` onto the doc; `Rig::from_doc()` reads and validates them.
`savePreset` / `loadPreset` therefore round-trip routings with no extra field
and no migration step. `Doc.routings` is `skip_serializing_if = "Vec::is_empty"`,
so a project with no routings is byte-identical to a pre-routing file.

The engine's state snapshot (`getState` / the `state` event) carries the same
list as `routings`, so the UI's copy is authoritative only as a cache.

## 7. Audio semantics (what the engine card must build)

A routing is realised at **compile time**, never by rewriting the saved model:

* `Rig::to_doc()` stays pure — it emits `routings` and nothing else. The saved
  file and the in-memory rig are unaffected by compilation.
* After `doc.build_in(...)`, the engine applies the enabled routings to the
  built `Graph` (new `Rig::apply_routings(&mut Graph, &Registry)` in
  `engine/src/channels.rs`, called from `engine::server::compile` **and**
  `app::ui::rebuild` so the egui dev app matches).
* For each enabled routing, in list order:
  1. find the edge currently driving `dest.block`'s `in` port;
  2. insert a `util.mix` node id `__routing_{routing.id}` (params `a = 1.0`,
     `b = 1.0`);
  3. if `gain != 1.0`, insert a `util.gain` node id `__routing_gain_{routing.id}`
     with `gain` (dB) `= 20 * log10(clamp(gain, 1e-3, 15.85))`;
  4. disconnect `dest.block.in`, then wire
     `old driver → mix.in_a`, `source.block.out → gain.in` (or `→ mix.in_b` when
     unity), `gain.out → mix.in_b`, `mix.out → dest.block.in`.
  Chaining is automatic: a second routing into the same block finds the first
  mix as the current driver and nests. Deterministic order = `routings` order.
  The `__routing_` / `__routing_gain_` prefixes are derived from the reserved
  `__master` / `__split` convention and cannot collide with a block id (block
  ids come from the palette's display names, which never start with `__`);
  `apply_routings` still returns an error rather than panicking if a node id is
  already taken.
* `Graph` needs one new method: `disconnect_input(node, port)` (mirror of
  `Doc::disconnect_input`). Nothing else in `dsp-graph` changes.
* Disabled routing ⇒ no node, no edge: the source returns to its original
  behaviour exactly.
* Because the tap is an ordinary graph edge, the source block's own downstream
  edge is untouched and the source can feed any number of routings.

Acceptance for the engine card: with `A → B`, `A`'s original output still
sounds **and** `B` receives `A`'s signal; disabling restores the original
path; a cycle attempt is refused by `createRouting` and can never reach the
graph; the fan-out is audible, not just drawn.

## 8. UI contract

`ui/src/protocol.ts`:

```ts
export interface RoutingEndpoint { channel: string; block: string }
export interface Routing { id: string; source: RoutingEndpoint; dest: RoutingEndpoint;
                           enabled: boolean; gain: number }
// State gains: routings: Routing[]
```

`ui/src/state.ts` gains `createRouting(source, dest)`, `removeRouting(id)`,
`setEnabled(id, enabled)`, `setGain(id, gain)`, `listRoutings()` — all through
the existing `mutate()` helper, so a refused command surfaces in `notice` and
the snapshot stays authoritative.

The routing line is a **read-only view of engine state**: it never edits
`channels` or block order. A line is selectable (click) and its selection is UI
state only (e.g. `selectedRouting`), never sent to the engine.

## 9. Non-goals

* No new channel add/remove commands; the four slots are fixed.
* No routing *ports* on blocks beyond the existing single `out` / `in`.
* No per-routing pan/phase/latency compensation.
* No migration tooling: the `serde` defaults are the migration.
