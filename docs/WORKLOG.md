# Worklog

The durable continuation point for this project. Any session — me or a cron
spawned worker — reads this before touching the code.

**STATUS: IN PROGRESS**

## Focus

**UI visual overhaul + state-delivery hardening (this session, uncommitted at
the time of writing).** Colour is now per category (`ui/src/theme.ts` sets a
hue per `Category`; cards, rack items, icons and the editor header all derive
from `--cat-h`). Knobs carry min/max end-stop labels. `choice` parameters
render as a segmented control (or a dropdown above five options) instead of a
knob, and the editor is a two-column grid so the controls use the whole width.
The selected card is highlighted, and a signal-path line with port dots runs
behind the chain.

The freeze the user reported is addressed on the delivery path: the engine now
bumps a `rev` counter on every rebuild and includes it in the snapshot, and it
broadcasts `event: state` to **every** client including the requester (the
reply carries the same state, so the client's `rev` guard makes the duplicate
a no-op). Command failures are no longer swallowed — a rejection surfaces as a
toast (`state.ts` `notice`). Pending requests are dropped when the socket
closes so their promises cannot hang.

**Previously: US-4.1's C ABI boundary is in place and verified from a real C
program.
`ffi/` (`pedal-ffi`) exposes the `dsp-graph` executor through a flat C
interface — `pedal_core_new` / `load_preset[_file]` / `set_sample_rate` /
`set_param_norm|real` / `get_param_norm|real` / `set_bypass` / `process` /
`reset` / `latency` / `foreach_node` / `foreach_param` — built as a
`staticlib` with `ffi/include/pedal_ffi.h` for the C++ shell to include.
`Executor` gained two read-only accessors (`param_norm`, `specs`) for it. The
graph is mono, so a stereo host passes interleaved stereo and gets the mono
signal duplicated to both channels; the deinterleave uses a scratch buffer
allocated in `pedal_core_new`, so `process` never allocates, and a block
larger than the declared `max_block` is refused rather than clamped.**

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
  ffi/       C ABI to the executor          (staticlib + header, for the plugin)
  engine/    engine process + library      (audio, channels, store, host, server)
  ui/        Preact + TypeScript + Vite    (shared by every shell)
  shell/     Tauri desktop shell           (done)
  plugin/    iPlug2 plugin (C++)            (later; links ffi/)
```

## Done

- `docs/UI-ARCHITECTURE.md` — the decision (webview shell, process split).
- `docs/ROADMAP.md` — epics and stories.
- `docs/IPC-PROTOCOL.md` — the wire contract.
- `engine/` — `audio`, `channels`, `store`, `host`, and the WebSocket IPC
  server (`server.rs`).
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

- Pi deployment prep: the engine now builds for the Pi and can boot with no
  audio device at all. `engine/cross-build.sh` produces a genuine
  `aarch64-unknown-linux-gnu` release binary from this Mac: it assembles an
  aarch64 Linux toolchain (messense/homebrew-macos-cross-toolchains), builds
  `alsa-lib` for aarch64 against that toolchain's sysroot, and links with
  `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER` plus `PKG_CONFIG_ALLOW_CROSS=1`
  (Rust's `pkg-config` crate refuses to cross-compile without a target sysroot).
  Verified: the output is a valid aarch64 ELF whose only non-libc dependency is
  `libasound.so.2`, which Raspberry Pi OS ships as `libasound2t64`;
  `pedal-dsp` and `dsp-graph` also `cargo check` clean for the target.
  `engine/src/headless.rs` adds a simulated device (`--headless`,
  `--source riff|live|silence`, `--bench <seconds>`, `--port <n>`) that runs the
  real `run_callback` on a paced device and **discards the output** — no audio
  hardware, and nothing is ever played. `engine/tests/headless_smoke.rs` spawns
  the real `pedal-engine --headless --port P --bench 3`, connects a real
  WebSocket client, checks the snapshot and a `setParam` round trip, and asserts
  the bench counters (`callbacks > 0`, `frames > 0`, `xruns == 0`). Verified:
  `cargo test -p pedal-engine --test headless_smoke` passes — on this host
  `callbacks=107 cpu=0.9% frames=27392 realtime=19.0 xruns=0` over 3 s (the low
  `realtime` is the macOS `sleep` overshoot; `cpu` is unaffected);
  `cargo test --workspace` passes (125 tests). `docs/PI-DEPLOY.md` is the
  runbook. The aarch64 binary itself cannot be executed here: qemu's linux-user
  emulation is Linux-host-only (`configure` refuses it on macOS), so its first
  boot is still pending real hardware.

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
- US-5.1 follow-up — the real `.nam` against the same Python reference:
  `graph/examples/nam_real_reference.rs` loads `models/MES DRECF RD Gain2 BAL
  O808 CAB.nam` through the real `Nam` block at its own 48 kHz (so the
  resampler is a pass-through and `output == model(input)`), renders 8192
  samples of a deterministic three-tone signal, and dumps `nam_input.raw` /
  `nam_output.raw`. `graph/tests/nam_reference.py --model <path.nam>` parses the
  `.nam` itself — SlimmableContainer (default full submodel), the layer-array
  weight layout, per-layer kernel/dilation/LeakyReLU, `layer1x1`, the multi-tap
  head rechannel, and `head_scale` — and runs the same forward pass in pure
  Python. Verified: max abs error 6.19e-07, rms 6.65e-08 over 8192 samples
  (PASS within 1e-5); the tiny-model path still passes at 9.24e-07 over 512
  samples; `cargo test --workspace` passes. Grouped convolutions, FiLM,
  post-stack heads and `condition_dsp` are rejected with a clear error rather
  than silently mis-run.
- US-4.1 step 1 — the C ABI boundary (`ffi/`, crate `pedal-ffi`): the executor
  behind a flat C interface, built as a `staticlib` with a header the C++ shell
  includes. `Executor` gained `param_norm` and `specs` (read-only) for it.
  `pedal_core_process` is mono in the graph, so a stereo host's interleaved
  input is deinterleaved into a pre-allocated scratch buffer and the mono
  output is duplicated to both channels — no allocation on the audio path, and
  an over-long block is refused instead of clamped. Verified: `cargo test -p
  pedal-ffi` (7 tests) — sample-exact parity between the ABI and a direct
  `Executor` run on the same preset (max abs error < 1e-6), param round trips in
  normalised and real space, bypass/reset, node/param enumeration, rate change
  recompiles the rig, and error paths return instead of panicking;
  `ffi/test-c.sh` compiles a C harness against the real staticlib and runs a
  220 Hz sine through `presets/04-rainbow.json` (7 nodes, peak 0.044, rms 0.025
  over 1024 frames, param 0.0000 → 0.2500), and also passes on `00-blank.json`
  and `06-nam-amp.json` (the latter loads the real `.nam`); `cargo test
  --workspace` passes.

## Next step

**US-4.1 continues with the iPlug2 shell.** The C ABI (`ffi/`) is done and
verified from C, so the next step is to scaffold `plugin/` with iPlug2, link
`target/release/libpedal_ffi.a`, and produce a `.vst3` that loads in a host.
Concretely: clone iPlug2, add a single VST3 target whose `process` calls
`pedal_core_process` and whose editor is the native fallback UI, then build the
bundle and load it in a headless VST3 host (Ableton Live 12 Suite is installed
on this machine, but launching it from cron would disturb the user — use the
SDK validator or a small host that discards output instead). Acceptance is
still "loads in a DAW and produces sound"; the C harness already proves the
second half of that against the real DSP.

The Pi deployment path is also still ready: connect the Pi 5 and follow
`docs/PI-DEPLOY.md` (`engine/cross-build.sh`, copy the binary, run
`pedal-engine --headless --bench 10` for the first US-5.4 CPU number; US-3.2
follows by attaching the interface). Until the Pi is reachable, US-3.1, US-3.2,
US-3.4 and US-5.4 stay blocked on hardware.

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
`npx --yes @tauri-apps/cli build` (tauri-cli 2.12.4). The identifier is
`app.pedal` — an earlier `com.pedal.app` tripped Tauri's "ends with `.app`"
warning. Changing it also moves the saved-state directory, so macOS forgets a
previously restored window frame.

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

**A cross-built aarch64 binary cannot be executed on this Mac.** qemu's
linux-user emulation (`qemu-aarch64`) is Linux-host-only — `configure` refuses
it on macOS ("linux-user not supported on this architecture"), and Homebrew's
`qemu` bottle ships only `qemu-system-*` (a full VM needs a kernel and rootfs).
Docker Desktop is not installed either. So `engine/cross-build.sh` can prove the
binary is a valid aarch64 ELF with the right `NEEDED` libraries, but its first
boot has to happen on the Pi. Do not claim the Pi build is "verified" beyond
that.

## How to run

```bash
# engine alone
./target/debug/pedal-engine --rig 04-rainbow

# real .nam against the Python reference (block-level parity)
cargo run -p dsp-graph --example nam_real_reference -- \
  "models/MES DRECF RD Gain2 BAL O808 CAB.nam" /tmp/nam-real
python3 graph/tests/nam_reference.py --model \
  "models/MES DRECF RD Gain2 BAL O808 CAB.nam" /tmp/nam-real

# the C ABI the VST3 shell links (C harness against the real staticlib)
./ffi/test-c.sh presets/04-rainbow.json

# self-contained desktop app + .dmg installer (spawns the engine, embeds the UI)
# CI=true skips the Finder cosmetic pass, which cannot run headless
cd shell && ./build-bundle.sh
open ../target/release/bundle/macos/Pedal.app
open ../target/release/bundle/dmg/Pedal_0.1.0_aarch64.dmg

# Pi 5: cross-build the engine, then boot it with no audio device
./engine/cross-build.sh
./target/aarch64-unknown-linux-gnu/release/pedal-engine --headless --bench 10
# see docs/PI-DEPLOY.md

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
