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

## Why the VST is different

`nih-plug-webview` embeds a wry webview in a `nih_plug` editor and does exactly
what we want. But it is explicitly **work in progress, not production-ready**:
tested on macOS, Windows hoped-for, no Linux.

The mature references are C++:

- **iPlug2** — a first-class `IWebViewEditorDelegate` with examples for raw
  HTML/CSS/JS, Svelte + Vite, and React + Vite. This is the proven pattern.
- **JUCE** — a first-class `WebBrowserComponent`.

So the technique is well established; in Rust it is early. Plan for a spike
that proves the webview-in-plugin path on macOS before committing to it, and
keep a native `egui` skin as the fallback. The fallback is a rendering layer
over the same parameter state, not a second engine.

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

## Process separation

A webview app puts the UI in a separate OS process from the Rust backend on
Linux (WebKitWebProcess), but the backend and the audio thread share one
process. **A backend crash takes audio down with it.**

For a floor pedal, consider splitting the engine into its own process and
letting the UI talk to it. That costs an IPC layer, but it means a UI bug
cannot silence the guitar, and the UI can be restarted without dropping audio.
Decide this before building the shell.

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
- **VST webview maturity.** `nih-plug-webview` is experimental. Spike it
  before committing; keep a native skin as the fallback.
- **UI/engine process separation.** A shared process means a UI crash can kill
  audio. Decide the split early — it is expensive to add later.
- **Two UIs to keep in sync** (web + native VST). Mitigated by shared design
  tokens and by the native skin being a rendering layer over the same state.
- **Touch on a 7" display.** Test with real touch, not a mouse; hover-only
  affordances do not exist on a pedal.

## First steps

1. **US-2.1** — this document (done).
2. **US-2.2** — spike: a Tauri shell rendering a trivial Preact page, on the
   Mac first. Prove the shell and the IPC.
3. **US-2.3** — IPC bridge with one parameter and one meter.
4. **US-2.10** — prove the UI is off the audio path before building anything
   else on top of it.
5. **US-4.1s** — separately, spike `nih-plug-webview` to find out whether the
   VST path is viable now.

The mock at `docs/ui-mock-neuraldsp.html` is the visual target.
