// The UI's single source of truth. Everything flows through the bridge; no
// component talks to the engine directly.

import { signal } from "@preact/signals";
import { createBridge, type Bridge } from "./bridge";
import { asState, isMeters, isState, type Meters, type State } from "./protocol";

export const bridge: Bridge = createBridge();

export const state = signal<State | null>(null);
export const meters = signal<Meters>({ in: 0, out: 0, cpu: 0, xruns: 0 });
export const ready = signal(false);
/** Fatal: the UI could not reach the engine at all. */
export const error = signal<string | null>(null);
/** Transient: the last command was rejected. Shown as a toast, never fatal. */
export const notice = signal<string | null>(null);
export const selected = signal<{ channel: string; block: string } | null>(null);
/** Whether the preset browser modal is open. */
export const presetsOpen = signal(false);

/** Highest revision applied. See `fold`. */
let lastRev = -1;

/**
 * Apply a snapshot, unless it is a duplicate or an out-of-order delivery.
 *
 * The engine sends the same state twice for a mutation (once in the reply, once
 * as a broadcast) and bumps `rev` on every rebuild, so `rev` is the ordering
 * key. A *lower* rev means the engine restarted and its counter began again —
 * accept that one, because the alternative is a UI that never recovers.
 */
function fold(next: State): boolean {
  if (next.rev === lastRev) return false;
  if (next.rev < lastRev) lastRev = -1;
  lastRev = next.rev;
  state.value = next;
  ready.value = true;
  return true;
}

/** Connect and take the first snapshot. Safe to call once. */
export async function init(): Promise<void> {
  bridge.subscribe((msg) => {
    if (isState(msg)) fold(asState(msg.payload));
    else if (isMeters(msg)) meters.value = msg.payload;
  });
  try {
    fold(asState(await bridge.request("getState")));
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

// --- Actions ----------------------------------------------------------------
// Mutating commands return the full new state, so there is nothing to merge.

/** Fold a command response into the snapshot. */
function apply(payload: unknown): void {
  if (!payload || typeof payload !== "object") return;
  const p = payload as Partial<State>;
  if (typeof p.rev === "number" && p.registry && Array.isArray(p.channels)) {
    fold(p as State);
  } else if (Array.isArray(p.presets) && state.value) {
    // A preset listing is a partial: patch just the names.
    state.value = { ...state.value, presets: p.presets };
  }
}

/**
 * Send a mutation. Failures are surfaced rather than swallowed: an unhandled
 * rejection here used to leave the UI silently showing stale state.
 */
const mutate = (cmd: string, args?: Record<string, unknown>) =>
  bridge
    .request(cmd, args)
    .then(apply)
    .catch((e: unknown) => {
      const msg = e instanceof Error ? e.message : String(e);
      // A dropped socket is not a command failure: the reconnect greeting
      // brings the authoritative state. Say nothing.
      if (msg !== "engine disconnected") notice.value = msg;
    });

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
