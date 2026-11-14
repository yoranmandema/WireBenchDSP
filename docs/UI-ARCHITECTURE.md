# UI Architecture

**Decision: the UI is a web app (Preact + TypeScript + CSS) rendered in a
native webview shell. No browser, no HTTP server.**

The UI is an ordinary web frontend built with Vite. Its static assets are
embedded into the Rust binary at build time and rendered by the operating
system's web engine inside a real native window:

- **macOS** — WKWebView
- **Windows** — WebView2
- **Linux / Pi 5** — WebKitGTK

The frontend talks to the Rust backend over an IPC bridge, not a network.

```
┌──────────────────────────────────────────────────────────┐
│  Native window (Tauri / wry+tao)                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Webview: Preact + TS + CSS                        │  │
│  │  left rack · top preset bar · chain · knob editor  │  │
│  └────────────────────────────────────────────────────┘  │
└───────────────────────────┬──────────────────────────────┘
                            │  IPC (direct function calls)
┌───────────────────────────▼──────────────────────────────┐
│  Rust backend                                             │
│   ├── DSP engine   (no_std core, real-time, never blocked) │
│   └── parameter state, presets, block registry             │
└───────────────────────────┬──────────────────────────────┘
                            │
                     audio interface
                   (USB or I2S codec)
```

## Why a webview and not a served page

A served page means the UI only exists inside a browser. The user wants a real
app: an icon in the dock, a borderless fullscreen window on the pedal's
touchscreen, no browser chrome. A webview gives that while keeping the UI
authoring in web tech.

The trade-off: the UI lives only on the device's own screen. It cannot be
opened on a phone the way a web page can. If that becomes wanted later, the
same frontend can also be served over HTTP — the frontend code does not change,
only how it is delivered.

## The three shells

| Shell | Stack | Notes |
|---|---|---|
| **Desktop** (macOS/Windows/Linux) | Tauri, or a thin `wry`+`tao` shell | One codebase, real app bundle. |
| **Pi 5 pedal** | The same app, borderless fullscreen on the touchscreen | Needs `libwebkit2gtk-4.1`; confirmed running on Raspberry Pi OS (Debian 12, aarch64). |
| **VST3** | `nih_plug` + `nih-plug-webview` | The one shell where this is early. See below. |

Tauri is the faster path (scaffolding, IPC, window management). `wry`+`tao` is
what Tauri itself is built on, and is the lighter option if the Pi build needs
trimming — the frontend is shared either way.

## The plugin shell

The UI codebase is shared regardless of which shell hosts the plugin; only the
bridge differs. So the question is the shell, not the webview.

| Shell | Language | Formats | Licence | Webview |
|---|---|---|---|---|
| **JUCE 8/9** | C++ | VST3, AUv2/v3, AAX, CLAP, LV2, standalone | AGPLv3 or paid (free under $20k/yr revenue) | Good; Linux backend buggy |
| **iPlug2** | C++ | VST2/3, AUv2/v3, AAX, WAM, standalone | zlib-like (permissive) | Mature, with Svelte/React examples |
| **nih-plug** | Rust | VST3, CLAP, standalone | ISC, but VST3 bindings are GPLv3 | Experimental (WIP) |
| **nice-plug** | Rust | VST3, CLAP, standalone | ISC, MIT/Apache bindings | Unproven |

Notes that matter:

- **JUCE 8 added a real webview UI path.** `WebBrowserComponent::Options`
  has `withResourceProvider` (serve HTML/JS/CSS straight from C++ memory, like
  Tauri's embedded assets), `withNativeFunction`, and `withEventListener`. So
  the premise holds: JUCE is realistically implementable.
- **JUCE's Linux webview is genuinely poor.** It cannot be hidden reliably,
  `ResourceProvider` requests are not routed without native integration, and a
  failed page load can crash the browser subprocess. This does not matter if
  JUCE only ever hosts the plugin (macOS/Windows), but it rules JUCE out as
  the Pi's standalone shell — which is what Tauri is for.
- **JUCE's licence is the real cost.** AGPLv3, or paid above $20k/year. For a
  project that might become a product, iPlug2's zlib-like licence is much
  safer.
- **The C++ cost is the same for JUCE and iPlug2.** Either way the plugin
  shell is C++ and the Rust DSP core sits behind a C ABI. That boundary is
  thin (one function call per block), but it means a C++ toolchain, CMake, and
  an FFI layer.
- **nih-plug is in maintenance mode since May 2026.** Its community fork
  `nice-plug` is the active project and fixes the VST3 GPLv3 binding, but its
  webview story is unproven.

**Recommendation:** spike **iPlug2** first — permissive licence, mature
webview path, wide format coverage. Consider JUCE only if its ecosystem or a
specific format matters enough to accept the licence. Avoid nih-plug/nice-plug
for the webview path until it has shipped in a real plugin.

Whichever shell is chosen, keep a native UI as the fallback. It is a rendering
layer over the same parameter state, not a second engine.

**The C ABI is already built.** `ffi/` (`pedal-ffi`) compiles the executor to a
`staticlib` and ships `ffi/include/pedal_ffi.h`; the shell only calls
`pedal_core_*`. `ffi/test-c.sh` links a C harness against the real staticlib.

## The invariant that makes this safe

**The UI never touches the audio path.**

The audio thread runs the DSP engine and owns the interface. The UI sends
parameter commands; the backend applies them to the executor between blocks.
So:

- closing, reloading, or freezing the UI does not interrupt audio,
- a slow or crashed UI cannot cause xruns,
- a knob turn reaches the engine in bounded time, not dependent on rendering.

This is the separation the current egui app already has (`Cmd` channel + atomic
executor handoff); only the transport changes.

## Process separation — decided: split

The engine and the UI run as **separate processes**. The UI never touches the
audio path, and a UI crash or restart cannot silence the guitar.

```
┌──────────────────────┐        IPC (params / state)      ┌──────────────────┐
│  UI process          │  ────────────────────────────►  │  Engine process  │
│  Tauri webview       │  ◄────────────────────────────  │  audio thread    │
│  dies, restarts      │        meters / state           │  DSP core        │
└──────────────────────┘                                 └──────────────────┘
                                                                     │
                                                              audio interface
```

- **Transport:** WebSocket over loopback TCP on every platform — a webview
  cannot open a Unix domain socket. JSON. Localhost only.
- **Engine process:** owns the audio interface and the DSP core, exposes
  parameters, presets, and meters.
- **UI process:** renders, sends commands, displays meters. It can be killed
  and restarted without a dropout.

This is the same shape as PiPedal. It costs an IPC layer and a state-sync
protocol, but it is what makes the pedal robust and lets the UI be replaced
later (webview today, something else later) without touching the engine.

The VST is the exception: a plugin cannot spawn processes, so the engine runs
in-process there. The engine is a library either way, so this costs nothing
extra — only the transport differs.

## What does not change

`pedal-dsp` and `dsp-graph` are already `no_std` and contain no I/O. They stay
exactly as they are. Only a host layer is added to own the audio interface and
the parameter state.

The egui app remains the **developer/debug surface** and the reference for how
the engine behaves, even after the web UI becomes the product UI.

## Tradeoffs

| | Webview shell (Tauri) | egui (current) |
|---|---|---|
| Visual ceiling | High — CSS, gradients, real knobs | Medium — custom paint for everything |
| Iteration speed | Fast — edit and reload | Slow — recompile Rust |
| One UI across shells | Yes (Pi + desktop); VST needs a spike | No — Pi needs a separate UI |
| VST | Possible but early in Rust | Already native |
| Extra machinery | Frontend build + IPC | None |
| Binary / RAM | ~8 MB, ~170 MB | Larger binary, less RAM |

## Risks

- **WebKitGTK on the Pi.** Known rendering bugs; workarounds exist
  (`WEBKIT_DISABLE_DMABUF_RENDERER=1`, `WEBKIT_DISABLE_COMPOSITING_MODE=1`).
  Keep the UI CSS-based and avoid WebGL-heavy views.
- **IPC state sync.** Splitting the engine and UI means the UI must resync when
  it reconnects. Keep the protocol a full-state snapshot plus deltas, not just
  deltas, so a reconnecting UI can always catch up.
- **Plugin shell maturity.** `nih-plug-webview` is experimental and
  `nih-plug` is in maintenance mode. Spike iPlug2 (or JUCE) before committing.
- **C++ and an FFI boundary.** Choosing iPlug2 or JUCE means a C++ toolchain,
  CMake, and a C ABI between the plugin shell and the Rust core.
- **JUCE licensing.** AGPLv3 or paid above $20k/year. iPlug2 avoids this.
- **Two UIs to keep in sync** (web + native VST fallback). Mitigated by shared
  design tokens and by the native skin being a rendering layer over the same
  state.
- **Touch on a 7" display.** Test with real touch, not a mouse; hover-only
  affordances do not exist on a pedal.

## First steps

1. **US-2.1** — this document (done).
2. **US-2.11** — the engine/UI IPC protocol and state-sync shape. Settled:
   WebSocket over loopback TCP (`docs/IPC-PROTOCOL.md`).
3. **US-2.2** — spike: a Tauri shell rendering a trivial Preact page on the
   Mac, talking to a stub engine process. Prove the shell, the IPC, and the
   reconnect path.
4. **US-2.3** — real IPC bridge with one parameter and one meter.
5. **US-2.10** — prove the UI is off the audio path before building anything
   else on top of it.
6. **US-4.4** — separately, spike the plugin shell (iPlug2 first) to find out
   whether the webview-in-plugin path is viable now.

The mock at `docs/ui-mock-neuraldsp.html` is the visual target.
