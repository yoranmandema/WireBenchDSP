# Worklog

The durable continuation point for this project. Any session — me or a cron
spawned worker — reads this before touching the code.

**STATUS: IN PROGRESS**

## Focus

**US-5.1 is verified against a self-contained Python reference: the tiny
WaveNet's forward pass, reimplemented in pure Python
(`graph/tests/nam_reference.py`), matches the Rust `Nam` block's output sample
by sample (max abs error 9.24e-07 over 512 samples). The Python `nam` package
is not available on this host — PyPI `nam` is a different project (Google's
Neural Additive Models), and NAM's own package needs torch — so the worklog's
fallback was used. The real OD808 `.nam` still loads and runs (`nam_check`:
peak 0.794, rms 0.186, no non-finite). Next: extend the Python reference to the
real SlimmableContainer `.nam` and compare that too.**

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
- US-3.3 (desktop app bundle): `npx @tauri-apps/cli build` produces
  `target/release/bundle/macos/Pedal.app` (plus a `.dmg`, which currently fails
  on this host). The Preact UI is embedded in the binary (`strings` finds
  `assets/index-*.js`). Verified end to end with no Vite dev server on 5173:
  the bundle's binary spawns `target/release/pedal-engine`, the engine listens
  on `ws://127.0.0.1:8737`, and the embedded UI connects and sends `getState`
  (see `/tmp/pedal-engine.log`). `engine_path()` now also looks next to the
  executable, so a bundle that ships the engine beside the shell works.
- Microphone permission + self-contained bundle (follow-ups 1–2, commit
  `0fe028c`): `shell/Info.plist` adds `NSMicrophoneUsageDescription` (Tauri
  auto-merges an `Info.plist` next to `tauri.conf.json`), and
  `bundle.externalBin` ships `target/release/pedal-engine` inside
  `Pedal.app/Contents/MacOS/`. `beforeBuildCommand` now also builds the release
  engine and copies the `-<host-triple>`-suffixed binary Tauri expects for
  cross-compilation. Verified: `npx @tauri-apps/cli build --bundles app` exits
  0; the fresh bundle has `Contents/MacOS/pedal-engine` and the mic key in
  `Info.plist`; launching the bundle's binary spawns that engine and the
  embedded UI connects (`client connected`, `<- getState`).
- Follow-up 3 (the `.dmg` installer): `shell/build-bundle.sh` runs
  `CI=true npx @tauri-apps/cli build`, which makes tauri-bundler pass
  `--skip-jenkins` and skip the Finder AppleScript that times out
  (`AppleEvent timed out. (-1712)`) in any session with no Finder GUI —
  cron, SSH, CI. The DMG still carries `Pedal.app` and the `Applications`
  symlink; only the cosmetic pass (background picture, icon layout) is
  skipped. Verified: exit 0; `hdiutil verify` reports VALID; the image
  mounts at `/Volumes/Pedal` with `Pedal.app` + `Applications` ->
  `/Applications`; launching the app *from the mounted volume* spawns the
  bundled engine and the embedded UI connects (`client connected`,
  `<- getState`).
- US-2.11 transport settled: the card's acceptance and `docs/UI-ARCHITECTURE.md`
  now say WebSocket over loopback TCP on every platform, matching
  `docs/IPC-PROTOCOL.md` and the code (`TcpListener::bind(("127.0.0.1", 8737))`
  in `engine/src/server.rs`, `WebSocketBridge` → `ws://127.0.0.1:8737` in
  `ui/src/bridge.ts`). A webview cannot open a Unix domain socket, so UDS is
  impossible for this UI. Verified: the only remaining "Unix domain" mentions
  in `docs/` are the ones explaining why it is not used.

- Kanban reconciliation: US-1.4–1.6, US-2.1–2.11, and US-3.3 are verified done
  in the worklog but still showed `ready`/`todo` on the board; marked done.
- US-5.2 (`graph/tests/pitch_delay.rs`): renders a 20 ms 220 Hz burst through a
  lone `PitchDelay` block (300 ms, 70% feedback, +600 cents, Target mode, 100%
  wet). The shifter sits in the feedback path, so each repeat is shifted again:
  the Hann-windowed DFT peak of each repeat lands at 220 → 311 → 440 → 622 Hz,
  within 15% of the expected `f0·2^(k·cents/1200)` and strictly increasing.
  Verified: `cargo test -p dsp-graph --test pitch_delay` passes; the full
  `cargo test --workspace` passes.
- US-5.3 (`graph/tests/pitch_sweep.rs`): the continuous sweep rate, through the
  real `PitchDelay` block. (1) `sweep_climbs_continuously_and_never_settles`
  renders a continuous 220 Hz sine with `feedback = 0` (a single pass, so the
  instantaneous pitch is measurable) in Sweep mode at 200 cents/s; the
  magnitude-squared spectral centroid lands at +102 / +196 / +300 cents at
  0.5 / 1.0 / 1.5 s — matching `rate·t` and strictly increasing, so it never
  settles. (2) `doubling_rate_doubles_the_climb`: the climb over 1.0 s is
  196 cents at 200 cents/s and 396 at 400 (ratio 2.0). (3)
  `target_mode_ignores_the_rate`: Target mode with `rate = 1200` holds +600
  cents and does not drift — the contrast that rules out a move-to-target
  glide. (4) `sweep_repeats_climb_in_the_feedback_loop`: a 20 ms burst with
  60% feedback gives repeats that rise monotonically and sit above the
  equivalent Target repeats (the sweep compounds across repeats). The shifter's
  crossfade spreads the spectrum into sidebands, so a plain peak pick is
  unreliable on a swept signal; the tests use the spectral centroid for the
  continuous case and a fine-grid (1 Hz) peak for the burst case. Verified:
  `cargo test -p dsp-graph --test pitch_sweep` passes (4 tests); the full
  `cargo test --workspace` passes (124 tests).
- US-5.1 (`graph/tests/nam_reference.py` + `graph/examples/nam_reference.rs`):
  the NAM block's output against a Python reference render. The card's
  acceptance is "Output matches a Python reference render of the same model";
  the Python `nam` package is unavailable on this host (PyPI `nam` is Google's
  Neural Additive Models, not Neural Amp Modeler; NAM's own `nam` needs torch,
  which is not installed), so the worklog's fallback was used: a pure-Python
  reimplementation of the tiny WaveNet forward pass (rechannel → dilated conv →
  input mixer → sum → ReLU → head accumulate → head rechannel → head_scale),
  compared sample by sample against the Rust `Nam` block on the same 512-sample
  sine. Max abs error 9.24e-07, rms 2.46e-07 — float32 rounding, well inside
  1e-5. The real `models/MES DRECF RD Gain2 BAL O808 CAB.nam` (a v0.7.0
  SlimmableContainer) still loads and runs: `nam_check` reports peak 0.794,
  rms 0.186, 0 non-finite, 3.89x dry-rms colour. Verified:
  `cargo run -p dsp-graph --example nam_reference` + `python3
  graph/tests/nam_reference.py`; `cargo test -p dsp-graph --test nam` (11
  passed); `cargo test --workspace` passes.

## Next step

**Extend the Python reference to the real OD808 `.nam`.** The tiny-vector
fallback satisfies the card's acceptance for the test model, but the user's
real model (`models/MES DRECF RD Gain2 BAL O808 CAB.nam`, a v0.7.0
SlimmableContainer) is the one that matters. Either install NAM's Python
package (needs torch — heavy) or extend `graph/tests/nam_reference.py` to
parse the SlimmableContainer's nested WaveNet and run the same forward pass
against the real weights, then compare sample by sample. The Rust side already
runs it (`nam_check`); only the independent reference is missing.

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

**Tauri runs `beforeDevCommand` / `beforeBuildCommand` with cwd = the
*frontend* directory (`dirs.frontend`), not the directory holding
`tauri.conf.json`** — here that is the workspace root, so `pnpm --dir ../ui
build` fails with "canonicalizing the `--dir` argument". The config uses the
explicit form `{ "script": "pnpm build", "cwd": "../ui" }`, which resolves
against the tauri dir.

**`cargo tauri` is not installed on this host.** The official JS wrapper is:
`npx --yes @tauri-apps/cli build` (tauri-cli 2.12.4). It warns that the
identifier `com.pedal.app` ends with `.app`.

**A GUI app launched from a cron/background session gets no window** — only a
long-lived shell had windows. To verify a bundle, run its binary directly and
watch `/tmp/pedal-engine.log` for `client connected` followed by
`<- getState`; that pair is the embedded UI's `init()` running.

**The DMG's Finder cosmetic pass fails in any session with no Finder GUI.**
`bundle_dmg.sh` drives Finder through AppleScript to lay out the volume; from
cron/SSH/CI that AppleEvent times out (`-1712`) or is denied (`-1743`). The
`.app` is already written by then, so `tauri build` exits non-zero even though
the app is fine. `shell/build-bundle.sh` sets `CI=true`, which makes
tauri-bundler pass `--skip-jenkins` and skip only that cosmetic pass — the DMG
still has the app and the `Applications` symlink. The trade-off is no custom
DMG background or icon positions.

## How to run

```bash
# engine alone
./target/debug/pedal-engine --rig 04-rainbow

# self-contained desktop app + .dmg installer (spawns the engine, embeds the UI)
# CI=true skips the Finder cosmetic pass, which cannot run headless
cd shell && ./build-bundle.sh
open ../target/release/bundle/macos/Pedal.app
open ../target/release/bundle/dmg/Pedal_0.1.0_aarch64.dmg

# debug build: still loads the Vite devUrl, so run `pnpm --dir ui dev` first
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
