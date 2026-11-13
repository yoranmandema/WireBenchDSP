# Worklog

The durable continuation point for this project. Any session — me or a cron
spawned worker — reads this before touching the code.

**STATUS: IN PROGRESS**

## Focus

**The Tauri shell is built and verified. Next: US-2.4–2.10 (UI features) —
US-2.4–2.8 are implemented in `ui/`; verify them, then build US-2.9 (preset
browser) and US-2.10 (prove the UI is off the audio path).**

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
  app/       egui dev tool                 (thin; re-exports the engine)
  engine/    engine process + library      (audio, channels, store, host, server)
  ui/        Preact + TypeScript + Vite    (shared by every shell)
  shell/     Tauri desktop shell           (NEXT)
  plugin/    iPlug2 plugin (C++)            (later)
```

## Done

- `docs/UI-ARCHITECTURE.md` — the decision (webview shell, process split).
- `docs/ROADMAP.md` — epics and stories.
- `docs/IPC-PROTOCOL.md` — the wire contract.
- `engine/` — `audio`, `channels`, `store`, `host`, and the WebSocket IPC
  server (`server.rs`). `app/` owns only the egui UI and re-exports the engine.
  `graph::Registry` is `Sync` so the server's async tasks can share it.
- `ui/` — Preact app: top bar (preset slots, source, meters, DSP load), left
  rack grouped by category, centre chain with drag-to-reorder, bottom knob
  editor. Speaks `docs/IPC-PROTOCOL.md` through a `Bridge` seam
  (`WebSocketBridge` today, `PluginBridge` for the plugin).
- Verified end to end: the engine binds `ws://127.0.0.1:8737`; a Node client
  reads the snapshot, changes a param, and adds a block; the browser UI shows
  the same state and the knob editor drives the engine. `cargo test` passes;
  `pnpm build` produces `ui/dist`.
- `shell/` — Tauri 2 desktop shell: hosts the Preact UI, spawns the engine
  process on launch, stops it on exit, and logs the engine to
  `/tmp/pedal-engine.log`. The CSP allows `ws://127.0.0.1:8737`. Verified:
  `cargo build` / `cargo test` pass; the running shell spawns `pedal-engine`,
  which serves the full snapshot on 8737. Debug builds use the Vite `devUrl`
  (standard Tauri behaviour); production embedding needs `tauri build` (the
  `custom-protocol` feature) — that is US-3.3.

## Next step

US-2.4–2.10: verify the implemented UI features (top bar, rack, chain, editor,
knob), then build US-2.9 (preset browser) and US-2.10 (prove the UI never
touches the audio path — kill the UI mid-play and show audio continues).

## How to run

```bash
# engine
./target/debug/pedal-engine --rig 04-rainbow

# desktop shell (spawns the engine itself; debug build uses the Vite dev server)
cd ui && pnpm dev          # http://localhost:5173
./target/debug/pedal-shell

# UI in dev (Vite proxies /engine to the engine)
cd ui && pnpm dev     # http://localhost:5173
```

## Rules

- Commit after every verified step so the stall watchdog stays quiet.
- Never run `hermes kanban specify` or `decompose` — they rewrite cards and
  dispatch workers.
- Unassigned kanban cards are safe; assigning one starts a worker.
- The UI never enters the audio path: it sends commands, the engine applies
  them between blocks.
- If another session is mid-edit in the working tree, do not overwrite its
  uncommitted files; verify and commit them instead.
