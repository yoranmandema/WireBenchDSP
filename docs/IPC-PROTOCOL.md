# IPC Protocol

The wire contract between the **UI** (Preact, in a webview) and the **engine**
(Rust, the process that owns the audio interface).

This protocol is shared by every shell:

| Shell | Transport | Bridge |
|---|---|---|
| Desktop / Pi (Tauri) | WebSocket over loopback TCP | `WebSocketBridge` |
| Dev / browser | WebSocket over loopback TCP | `WebSocketBridge` |
| VST3 (iPlug2) | in-process C ABI | `NativeBridge` |

Only the bridge differs. The message format, the commands, and the state shape
are identical everywhere.

## Transport

- **WebSocket**, text frames, one JSON object per frame.
- **Endpoint:** `ws://127.0.0.1:8737/` (loopback only).
- The engine binds `127.0.0.1`; no external interface.
- A webview cannot open a Unix domain socket, so the transport is loopback TCP
  on every platform — including macOS and Linux.
- Binary frames are not used.

A frame is always a single JSON object. No batching, no arrays of messages.

## Message envelope

Every message has a `type`:

```jsonc
// Client -> server: a command, answered by exactly one `res` with the same id
{ "type": "req", "id": 7, "cmd": "setParam", "args": { ... } }

// Server -> client: the answer to a `req`
{ "type": "res", "id": 7, "ok": true, "payload": { ... } }
{ "type": "res", "id": 7, "ok": false, "error": "no such node" }

// Server -> client: unsolicited
{ "type": "event", "event": "state",    "payload": { ... } }
{ "type": "event", "event": "meters",   "payload": { ... } }
{ "type": "event", "event": "error",    "payload": { "message": "..." } }
```

Rules:

- `id` is chosen by the client, unique among in-flight requests. It is echoed
  back verbatim; the client correlates by id.
- `res.payload` depends on the command. For mutating commands it is the **full
  new state** — see below.
- Events are server-initiated. The client must handle them without having asked.

## Why full state instead of deltas

The whole rig state is a few kilobytes. Sending it after every mutation is
cheap, and it makes a reconnecting UI trivial: it never has to replay a delta
log, it just applies the latest snapshot.

Deltas would save bandwidth at a scale this UI will never reach, at the cost of
a much harder reconnection story. **Snapshot, not deltas.**

## Connection lifecycle

1. Client connects.
2. Server sends one `event: state` with the full snapshot (no request needed).
3. Client sends `req`s; server answers each with a `res` and, for mutations,
   broadcasts nothing — the answer carries the new state. (Other connected
   clients get an `event: state`.)
4. Server sends `event: meters` at 20 Hz while audio is running.
5. On disconnect the engine keeps running. On reconnect the client gets a
   fresh snapshot.

## State shape

```jsonc
{
  "preset": "04-rainbow",            // current preset name, "" if unsaved
  "presets": ["00-blank", "01-clean", "..."],
  "source": "riff",                  // "riff" | "live" | "silence"
  "registry": {                      // static: block types the UI can offer
    "amp.basic": {
      "name": "Amp",
      "category": "amp",             // amp | cab | drive | tone | delay | reverb | util | io
      "params": [
        { "id": "voice", "name": "Voice", "kind": "choice",
          "choices": ["Clean", "Crunch", "Lead"],
          "min": 0, "max": 2, "default": 1 },
        { "id": "gain",  "name": "Gain",  "kind": "number",
          "min": 0, "max": 10, "default": 5, "unit": "linear" }
      ]
    }
  },
  "channels": [
    {
      "id": "ch1",
      "name": "Channel 1",
      "volume": 0.0,                 // dB, -60..12
      "sources": ["in"],             // "in" | "ch1" | "ch2" | ...
      "blocks": [
        {
          "id": "amp",               // unique within the channel
          "type": "amp.basic",       // registry key
          "name": "Amp",             // display name (from the registry)
          "bypass": false,
          "params": { "voice": 0, "gain": 3.5, "bass": 0, "mid": 0,
                      "treble": 0, "level": -5.0 }
        }
      ]
    }
  ]
}
```

`registry` is static for a given engine build; the UI may cache it.

## Commands

All commands are `req` messages. Mutating commands return the full state in
`payload`. Non-mutating commands return their specific payload.

| Cmd | args | Effect | payload |
|---|---|---|---|
| `getState` | — | — | full state |
| `setParam` | `{channel, block, param, value}` | set one parameter | full state |
| `setMasterVolume` | `{channel, value}` | channel volume (dB) | full state |
| `setBypass` | `{channel, block, bypass}` | bypass a block | full state |
| `addBlock` | `{channel, type, name, pos}` | append a block | full state |
| `removeBlock` | `{channel, block}` | remove a block | full state |
| `moveBlock` | `{channel, block, to}` | reorder | full state |
| `addChannel` | — | append a channel | full state |
| `removeChannel` | `{channel}` | remove a channel | full state |
| `setSources` | `{channel, sources}` | channel inputs | full state |
| `setSource` | `{source}` | riff / live / silence | full state |
| `listPresets` | — | — | `{"presets": [...]}` |
| `loadPreset` | `{name}` | load a preset | full state |
| `savePreset` | `{name}` | save the current rig | full state |
| `deletePreset` | `{name}` | delete a preset | full state |
| `renamePreset` | `{from, to}` | rename a preset | full state |

Errors return `{ "ok": false, "error": "..." }`. A failed mutation never
changes state, so the client can keep its current snapshot.

## Meters

```jsonc
{ "type": "event", "event": "meters",
  "payload": { "in": 0.42, "out": 0.38, "cpu": 12.3, "xruns": 0 } }
```

- `in`, `out`: peak levels, 0..1, held with a fast attack and slow release.
- `cpu`: percentage of one core used by the DSP, measured over the last second.
- `xruns`: cumulative buffer underrun count.

20 Hz is enough for a meter; it is not a control path.

## Real-time safety

- Parameter changes are applied by the **audio thread** between blocks. The UI
  never runs in the callback.
- Structural changes (add/remove/reorder/load preset) compile a new executor
  and queue it; the audio thread swaps at a block boundary. No gap, no glitch.
- The UI process may be killed at any moment. The engine does not depend on it.

## Versioning

The protocol has no version field. Both sides ship together; the state shape
is additive (new fields are ignored by older clients). If a breaking change is
ever needed, it gets a new port and a new `protocol` crate.
