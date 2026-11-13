# Worklog

The durable continuation point for this project. Any session — me or a cron
spawned worker — reads this before touching the code.

**STATUS: IN PROGRESS**

## Focus

**US-2.11 / US-2.2** — the engine/UI split and the webview UI.

## Goal

Three shells, one HTML/Preact frontend:

| Shell | Host | Engine |
|---|---|---|
| Desktop (macOS/Win/Linux) | Tauri webview | separate engine process over WebSocket |
| Pi 5 pedal | Tauri webview, borderless | same engine process |
| VST3 | iPlug2 (C++) | in-process |

## Layout

```
pedal-dsp/
  dsp/       no_std DSP primitives          (exists)
  graph/     block graph + executor        (exists)
  app/       egui dev tool                 (exists)
  engine/    NEW — engine process + library (state, audio, IPC server, C ABI)
  ui/        NEW — Preact + TypeScript     (shared by every shell)
  shell/     NEW — Tauri desktop shell
  plugin/    NEW — iPlug2 plugin (C++)
```

## Done

- `docs/UI-ARCHITECTURE.md` — the decision (webview shell, process split).
- `docs/ROADMAP.md` — epics and stories.
- `docs/ui-mock-neuraldsp.html` — the visual target.
- `docs/IPC-PROTOCOL.md` — the wire protocol.
- `engine/` crate — `audio`, `channels`, `store`, `host`, and the WebSocket IPC
  server (`server.rs`). `app/` now re-exports the engine modules and owns only
  the egui UI. `graph::Registry` is `Sync` so the server's async tasks can share
  it.
- Verified: `cargo build` + `cargo test` (116 tests) clean; `pedal-app
  --selftest` renders 192k frames with 0 xruns; the engine process binds
  `ws://127.0.0.1:8737` and a two-client probe confirmed the greeting snapshot,
  `getState`/`setParam`/`listPresets` round-trips, 20 Hz meters, and that a
  mutation broadcasts `event:state` to the other client only.

## Next step

Build the `ui/` crate: Vite + Preact + TypeScript, matching
`docs/ui-mock-neuraldsp.html`, with a `WebSocketBridge` that speaks
`docs/IPC-PROTOCOL.md`. Then the `shell/` Tauri wrapper (US-2.2) and the first
real bridge wiring (US-2.3: one param, one meter).

`ui/` is already scaffolded in the working tree but is **not committed** —
another session is writing it. Verify it (`pnpm build`) before building on it.

## Rules

- `touch docs/.heartbeat` at the start of every work turn. The stall watchdog
  only fires when the heartbeat is stale AND this file says IN PROGRESS.
- Never run `hermes kanban specify` or `decompose` — they rewrite cards and
  dispatch workers.
- Unassigned kanban cards are safe; assigning one starts a worker.
- Commit after every verified step so the watchdog can see progress.
