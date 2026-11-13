# UI Architecture

**Decision: a web page served by the Rust host, for the standalone shells. A
native UI for the VST.**

The UI is a static frontend the Rust host serves over HTTP on localhost. It
talks to the engine over a WebSocket. This is the same shape as PiPedal, and
it is what makes one UI code base cover the pedal, the desktop app, and the
design language the VST skin follows.

```
┌──────────────────────────────────────────────────────────┐
│  Browser / phone / tablet / 7" touchscreen                │
│  left rack · top preset bar · signal chain · knob editor  │
└───────────────────────────┬──────────────────────────────┘
                            │  HTTP (page)  +  WebSocket (state)
                            │  localhost only
┌───────────────────────────▼──────────────────────────────┐
│  Rust host                                                │
│   ├── DSP engine   (no_std core, real-time, never blocked) │
│   └── tiny HTTP + WS server, static assets embedded        │
└───────────────────────────┬──────────────────────────────┘
                            │
                     audio interface
                   (USB or I2S codec)
```

## The three shells

| Shell | How the UI loads | Notes |
|---|---|---|
| **Pi 5 pedal** | Headless service; any device on the pedal's WiFi opens the page | The primary target. No window manager needed. |
| **Desktop (macOS/Windows/Linux)** | A thin native window loading the served page | `wry`/`tao` for a real app, or just open the default browser. Tauri works but drags in a system webview the Pi does not need. |
| **VST3** | **Native UI**, not the page | A plugin host will not reliably run a web server, and a plugin should not open sockets. See below. |

## Why not a web view inside the VST

A VST3 plugin is a shared library loaded into someone else's DAW process.
Running an HTTP server and a browser engine there means:

- a listening socket inside a DAW process (some hosts and users object),
- a webview dependency per plugin format and platform,
- scan/security friction, and a much larger binary.

`iPlug2` and JUCE both offer a web view component, but it is the fragile path.
The practical answer is a compact native UI that **mirrors the web UI's design
language** (same colours, same layout regions, same knob geometry) so the two
feel like one product. The DSP core, the block registry, and the preset format
are shared, so the native skin is a rendering layer, not a second engine.

If a web view in the plugin becomes a hard requirement later, the frontend can
be reused inside it — but it should be treated as a later spike, not the plan.

## Protocol

- **HTTP** serves the page and its assets. Assets are embedded in the binary
  (`include_str!` / `include_dir!`), so there is no deploy step and no
  separate web server to run.
- **WebSocket** carries everything else, both directions:
  - UI → host: parameter changes, preset load/save/rename, block
    add/remove/reorder, bypass.
  - Host → UI: meter levels, current preset, engine state, error notices.

Localhost only, bound to `127.0.0.1`. On the Pi, the service is reachable from
the pedal's own network; that is the one place a real network exposure exists,
so it needs to be unauthenticated-but-isolated (or a simple PIN) and never
exposed beyond the pedal's WiFi.

## The invariant that makes this safe

**The UI never touches the audio path.**

The audio thread runs the DSP engine and owns the interface. The UI sends
commands; the host applies them to the executor between blocks. So:

- closing, reloading, or freezing the UI does not interrupt audio,
- a slow or disconnected UI cannot cause xruns,
- the round-trip latency of a knob turn is bounded by one audio block, not by
  network conditions.

This is the same separation the current egui app already has (`Cmd` channel +
atomic executor handoff); only the transport changes.

## What does not change

`pedal-dsp` and `dsp-graph` are already `no_std` and contain no I/O. They stay
exactly as they are. Only the host layer is added to:

1. own the audio interface,
2. own the HTTP/WS server,
3. translate UI commands into `Cmd` messages.

The egui app remains useful as the **developer/debug surface** and as the
reference for how the engine behaves, even after the web UI becomes the product
UI.

## Tradeoffs

| | Served web UI | egui (current) |
|---|---|---|
| Visual ceiling | High — CSS, gradients, real knobs | Medium — custom paint for everything |
| Iteration speed | Fast — edit and reload | Slow — recompile Rust |
| One UI across shells | Yes, for standalone | No — Pi needs a separate UI |
| VST | Needs a native skin anyway | Already native |
| Extra machinery | HTTP + WS + build step | None |
| Binary size | Smaller (no egui) | Larger |

## Risks

- **Build step for the frontend.** Mitigated by embedding prebuilt assets; the
  page can also be opened directly from disk during development.
- **Touch latency on the Pi.** A local WebSocket is sub-millisecond; the real
  risk is the render loop, not the transport.
- **Two UIs to keep in sync.** Mitigated by the shared design tokens and by the
  native VST skin being a rendering layer over the same state.
- **Discovery on the pedal's network.** Needs a stable hostname
  (`pedal.local`) and a documented way to connect.

## First steps

1. **US-2.1** — this document (done).
2. **US-2.2** — serve a trivial static page from the host; prove the binary
   works with no separate server.
3. **US-2.3** — WebSocket link with one parameter and one meter.
4. **US-2.10** — prove the UI is off the audio path before building anything
   else on top of it.

The mock at `docs/ui-mock-neuraldsp.html` is the visual target.
