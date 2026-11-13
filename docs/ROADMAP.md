# Roadmap

Epics and user stories for the pedal. The kanban board (`pedal`) carries the
stories as cards; this file is the map that groups them.

Status legend: **done** · **ready** (on the board, unassigned) · **planned**

---

## E1 · Concept POC on macOS (egui) — done

Prove the whole chain works — open the app, hear sound, add a block, hear it
change — before investing in a real UI or hardware.

| Story | Title | Status |
|---|---|---|
| US-1.1 | Blank pedalboard is a true bypass | done |
| US-1.2 | Adding a block changes the sound | done |
| US-1.3 | Reorder blocks by dragging | done |
| US-1.4 | Select a block and edit its params | done |
| US-1.5 | Route live guitar input | done |
| US-1.6 | Save and recall a preset | done |

Verified by `app/tests/rig.rs` and `app/tests/layout.rs`.

---

## E2 · Web UI (Neural DSP-style)

A web page served by the Rust host: better visuals, faster iteration, and one
UI that runs on the Pi and the desktop. Decision in `docs/UI-ARCHITECTURE.md`,
visual target in `docs/ui-mock-neuraldsp.html`.

| Story | Title | Status |
|---|---|---|
| US-2.1 | Architecture spike: webview shell vs served page | done (`docs/UI-ARCHITECTURE.md`) |
| US-2.2 | Native webview shell (Tauri) rendering a Preact page | ready |
| US-2.3 | IPC bridge: params and meters | ready |
| US-2.4 | Top bar: preset name, slots, meters | ready |
| US-2.5 | Left rack: amp, cab, FX selection | ready |
| US-2.6 | Centre chain with In/Out bookends | ready |
| US-2.7 | Bottom editor for the selected block | ready |
| US-2.8 | Rotary knob component | ready |
| US-2.9 | Preset browser | ready |
| US-2.10 | UI never touches the audio path | ready |
| US-2.11 | Engine/UI process split | ready |

Dependencies: 2.2 → 2.3 → {2.4, 2.5, 2.6, 2.7, 2.9}; 2.6 → 2.7 → 2.8; 2.10
and 2.11 should be settled early, before anything is built on the transport or
the process layout.

---

## E3 · Standalone shells (Pi 5 + desktop)

Run the same UI as a real product: headless on the pedal, windowed on the
desktop.

| Story | Title | Status |
|---|---|---|
| US-3.1 | Pi 5 native fullscreen window on the touchscreen | ready (needs 2.2) |
| US-3.2 | Pi 5 audio I/O | ready |
| US-3.3 | Desktop app bundle | ready (needs 2.2) |
| US-3.4 | Touchscreen layout | ready (needs 2.6) |

---

## E4 · VST3 plugin

Ship the same engine as a DAW plugin. The plugin gets a native skin; see
`docs/UI-ARCHITECTURE.md` for why a web view is not the plan.

| Story | Title | Status |
|---|---|---|
| US-4.1 | VST3 wrapper | ready |
| US-4.2 | Spike: webview inside the VST3 plugin | ready (needs 4.1) |
| US-4.3 | DAW automation and preset recall | ready (needs 4.2) |

---

## E5 · DSP engine and NAM

The sound: real model playback and the pitch-shifting delay.

| Story | Title | Status |
|---|---|---|
| US-5.1 | Run a .nam model in real time | ready |
| US-5.2 | Pitch-shifting delay | ready |
| US-5.3 | Continuous sweep rate (cents/sec) | ready (needs 5.2) |
| US-5.4 | Pi 5 CPU headroom and latency | ready |

---

## E6 · Hardware

The physical pedal: analogue front end, conversion, controls, enclosure.

| Story | Title | Status |
|---|---|---|
| US-6.1 | Analogue front end | ready |
| US-6.2 | ADC/DAC choice | ready |
| US-6.3 | Footswitches and expression pedal | ready |
| US-6.4 | Enclosure, power, and thermal | ready |
| US-6.5 | Display | ready |

---

## How the board works

```bash
hermes kanban list                      # see the board
hermes kanban show t_xxx                # read a card
hermes kanban assign t_xxx default      # route it to a profile
hermes kanban dispatch                  # run ready cards now
```

Unassigned `ready` cards are skipped by the dispatcher, so the backlog is safe
to sit on. Assigning a card to a profile is what makes it run.

Cards that depend on another card should link with
`hermes kanban link <parent> <child>`; the child stays parked until the parent
is done. Do **not** link a support card to the card it is meant to unblock —
that gates the support card behind its own blocker.

Full reference: <https://hermes-agent.nousresearch.com/docs/user-guide/features/kanban>
