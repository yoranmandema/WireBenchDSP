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

export const setParam = (channel: string, block: string, param: string, value: number) =>
  bridge.request("setParam", { channel, block, param, value });

export const setMasterVolume = (channel: string, value: number) =>
  bridge.request("setMasterVolume", { channel, value });

export const setBypass = (channel: string, block: string, bypass: boolean) =>
  bridge.request("setBypass", { channel, block, bypass });

export const addBlock = (channel: string, type: string, name: string) =>
  bridge.request("addBlock", { channel, type, name });

export const removeBlock = (channel: string, block: string) =>
  bridge.request("removeBlock", { channel, block });

export const moveBlock = (channel: string, block: string, to: number) =>
  bridge.request("moveBlock", { channel, block, to });

export const addChannel = () => bridge.request("addChannel");

export const removeChannel = (channel: string) =>
  bridge.request("removeChannel", { channel });

export const setSources = (channel: string, sources: string[]) =>
  bridge.request("setSources", { channel, sources });

export const setSource = (source: "riff" | "live" | "silence") =>
  bridge.request("setSource", { source });

export const loadPreset = (name: string) => bridge.request("loadPreset", { name });

export const savePreset = (name: string) => bridge.request("savePreset", { name });

export const select = (channel: string, block: string) => {
  selected.value = { channel, block };
};
