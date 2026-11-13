# Worklog

The durable continuation point for this project. Any session — me or a cron
spawned worker — reads this before touching the code.

**STATUS: IN PROGRESS**

## Focus

**US-2.10 is implemented and verified: the UI never touches the audio path.
Next: US-3.3 — build the self-contained desktop app (`cargo tauri build`) and
prove the embedded UI runs with no Vite dev server.**

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
- `ui/` preset browser (US-2.9): a modal listing every preset with load, save,
  rename, and delete. The engine gained a `renamePreset` command, and
  `savePreset` / `deletePreset` / `renamePreset` now return the full state
  (matching the protocol's own rule), so the UI folds one snapshot into
  `state.ts` instead of merging. Verified: `pnpm build`; 13/13 WebSocket
  command checks against a scratch preset dir; 9/9 headless-Chrome checks of
  the built UI (boot, list, highlight current, load rebuilds the chain,
  close).
- US-2.10 (`engine/tests/ui_off_audio_path.rs`): the UI is off the audio path,
  against the real engine code. (1) `killing_the_ui_does_not_disturb_audio`
  runs the real `run_callback` on a simulated device (separate input and
  output threads over the real ring), the real IPC server on an ephemeral
  port, and a real WebSocket client. The client is frozen, then dropped; audio
  keeps flowing at full real-time pace (98–100% of 48 kHz), xruns stay at
  zero, the output stays non-silent, and a second client reconnects and gets
  the full snapshot with the dead UI's knob move intact. (2)
  `a_command_lands_within_one_block` uses a gate block to issue a `SetParam`
  in the middle of a block: block 1 still carries the old value and block 2
  already carries the new one, so worst-case UI-to-sound latency is exactly
  one block (64 frames = 1.33 ms at 48 kHz). `server::serve` split into
  `serve` + `serve_listener` so the test can bind an ephemeral port.
- `shell/` — Tauri 2 desktop shell: hosts the Preact UI, spawns the engine
  process on launch, stops it on exit, and logs the engine to
  `/tmp/pedal-engine.log`. The CSP allows `ws://127.0.0.1:8737`. Verified:
  `cargo build` / `cargo test` pass; the running shell spawns `pedal-engine`,
  which serves the full snapshot on 8737. Debug builds use the Vite `devUrl`
  (standard Tauri behaviour); production embedding needs `tauri build` (the
  `custom-protocol` feature) — that is US-3.3.

## Next step

US-3.3: `cargo tauri build` (release, `custom-protocol`) and prove the app is
self-contained — launch the `.app` with no Vite dev server running and show
the webview really renders the UI (check `/tmp/pedal-engine.log` for a client
connection, or `CGWindowListCopyWindowInfo`), and that the embedded UI still
reaches the engine it spawns on `ws://127.0.0.1:8737` (the CSP already allows
it). Debug builds stay dev builds and load `devUrl`, so this needs the
feature, not just `cargo build`.

Open question for US-2.11: its card asks for a Unix domain socket on
macOS/Linux, but a webview can only open a WebSocket over TCP, and
`docs/IPC-PROTOCOL.md` already commits to loopback TCP. Confirm that decision
before building anything on it.

## Gotcha worth remembering

**`std::thread::sleep` cannot pace a simulated device on this host.** Measured
2026-11-13: a 5 ms sleep takes ~21 ms, 10 ms ~56 ms, 100 ms ~211 ms. Any test
that needs a real-time device period must spin on `Instant::now()` (see
`Pacer` in `engine/tests/ui_off_audio_path.rs`). `tokio::time::sleep` behaves
better, but measure the window with `Instant::elapsed()` and scale the
expectation rather than assuming the requested duration.

**Port 8737 can be held by a stale engine from an earlier session.** Check
`lsof -nP -iTCP:8737 -sTCP:LISTEN` before testing; a new engine exits silently
when the bind fails, and tests then talk to the old binary (which may not know
new commands). Kill the old PID first.

**A Tauri debug build is always a dev build.** `tauri`'s build script sets
`dev = !custom-protocol`, so `cargo build` (no features) makes the app load
`devUrl` (the Vite dev server) and show a blank window when it is not running.
A self-contained app needs:

```bash
cargo build -p pedal-shell --features custom-protocol   # or cargo tauri build
```

Do not debug a blank Tauri window through `System Events` — a bare binary is
not in the accessibility database, so it reports 0 windows even when the window
exists. Use `CGWindowListCopyWindowInfo`, or check whether the engine log shows
a client connection.

## How to run

```bash
# engine alone
./target/debug/pedal-engine --rig 04-rainbow

# self-contained desktop app (spawns the engine itself, embeds the UI)
cargo build -p pedal-shell --features custom-protocol
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
