// The UI's single source of truth. Everything flows through the bridge; no
// component talks to the engine directly.

import { signal } from "@preact/signals";
import { createBridge, type Bridge } from "./bridge";
import { asState, isMeters, isState, type Meters, type State } from "./protocol";

export const bridge: Bridge = createBridge();

export const state = signal<State | null>(null);
export const meters = signal<Meters>({ in: 0, out: 0, cpu: 0, xruns: 0 });
export const ready = signal(false);
export const error = signal<string | null>(null);
export const selected = signal<{ channel: string; block: string } | null>(null);
/** Whether the preset browser modal is open. */
export const presetsOpen = signal(false);

/** Connect and take the first snapshot. Safe to call once. */
export async function init(): Promise<void> {
  // The title doubles as a headless diagnostic: `window.title()` reads it.
  document.title = "Pedal (connecting)";
  bridge.subscribe((msg) => {
    if (isState(msg)) {
      state.value = msg.payload;
      ready.value = true;
      document.title = `Pedal - ${msg.payload.preset}`;
    } else if (isMeters(msg)) {
      meters.value = msg.payload;
    }
  });
  try {
    state.value = asState(await bridge.request("getState"));
    ready.value = true;
    document.title = `Pedal - ${state.value.preset}`;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
    document.title = "Pedal (offline)";
  }
}

// --- Actions ----------------------------------------------------------------
// Mutating commands return the full new state, so there is nothing to merge.

/** Fold a command response into the snapshot. */
function apply(payload: unknown): void {
  if (!payload || typeof payload !== "object") return;
  const p = payload as Partial<State>;
  if (p.registry && Array.isArray(p.channels)) {
    state.value = p as State;
    document.title = `Pedal - ${state.value.preset}`;
  } else if (Array.isArray(p.presets) && state.value) {
    state.value = { ...state.value, presets: p.presets };
  }
}

const mutate = (cmd: string, args?: Record<string, unknown>) =>
  bridge.request(cmd, args).then(apply);

export const setParam = (channel: string, block: string, param: string, value: number) =>
  mutate("setParam", { channel, block, param, value });

export const setMasterVolume = (channel: string, value: number) =>
  mutate("setMasterVolume", { channel, value });

export const setBypass = (channel: string, block: string, bypass: boolean) =>
  mutate("setBypass", { channel, block, bypass });

export const addBlock = (channel: string, type: string, name: string) =>
  mutate("addBlock", { channel, type, name });

export const removeBlock = (channel: string, block: string) =>
  mutate("removeBlock", { channel, block });

export const moveBlock = (channel: string, block: string, to: number) =>
  mutate("moveBlock", { channel, block, to });

export const addChannel = () => mutate("addChannel");

export const removeChannel = (channel: string) => mutate("removeChannel", { channel });

export const setSources = (channel: string, sources: string[]) =>
  mutate("setSources", { channel, sources });

export const setSource = (source: "riff" | "live" | "silence") =>
  mutate("setSource", { source });

export const listPresets = () => mutate("listPresets");

export const loadPreset = (name: string) => mutate("loadPreset", { name });

export const savePreset = (name: string) => mutate("savePreset", { name });

export const deletePreset = (name: string) => mutate("deletePreset", { name });

export const renamePreset = (from: string, to: string) =>
  mutate("renamePreset", { from, to });

export const select = (channel: string, block: string) => {
  selected.value = { channel, block };
};
