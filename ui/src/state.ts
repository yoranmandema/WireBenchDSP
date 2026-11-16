// The UI's single source of truth. Everything flows through the bridge; no
// component talks to the engine directly.

import { signal } from "@preact/signals";
import { createBridge, type Bridge } from "./bridge";
import { type BlockPayload } from "./dnd";
import { asState, isMeters, isState, type Meters, type Routing, type RoutingEndpoint, type State } from "./protocol";

export const bridge: Bridge = createBridge();

export const state = signal<State | null>(null);
export const meters = signal<Meters>({ in: 0, out: 0, cpu: 0, xruns: 0 });
export const ready = signal(false);
/** Fatal: the UI could not reach the engine at all. */
export const error = signal<string | null>(null);
/** Transient: the last command was rejected. Shown as a toast, never fatal. */
export const notice = signal<string | null>(null);
export const selected = signal<{ channel: string; block: string } | null>(null);
/** The selected cross-channel routing line, if any. UI-only: the engine has no
 *  notion of selection and is never told about it. */
export const selectedRouting = signal<string | null>(null);
/** Whether the preset browser modal is open. */
export const presetsOpen = signal(false);
/** Channel the add-block picker targets; null when the picker is closed. */
export const addBlockFor = signal<string | null>(null);
/** Current filter text of the add-block picker. */
export const blockQuery = signal("");

/** Open the picker over `channel`, resetting any previous search. */
export function openBlockPicker(channel: string): void {
  blockQuery.value = "";
  addBlockFor.value = channel;
}

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
  // A snapshot means the engine is reachable; drop any boot-time error so a
  // slow-starting engine doesn't leave the UI stuck on the error screen.
  error.value = null;
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
    const msg = e instanceof Error ? e.message : String(e);
    // The engine may still be starting (the shell spawns it alongside the
    // window). The bridge keeps reconnecting and the engine greets every new
    // connection with a snapshot, which `fold` picks up, so keep showing
    // "Connecting…" instead of the error screen.
    if (msg !== "engine disconnected") error.value = msg;
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
  } else if (Array.isArray(p.routings) && state.value) {
    // `listRoutings` answers with the list alone; the snapshot stays the
    // authority for everything else.
    state.value = { ...state.value, routings: p.routings as Routing[] };
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

/**
 * Insert a block dragged from the browser at `index` in `channel`.
 *
 * The engine's `addBlock` appends, so this adds and then moves the new node
 * into place. The new id is found by diffing the channel's block ids across the
 * add, which is robust to the engine's name-derived id suffixing (`amp`, `amp2`,
 * …). A drop on an empty channel is just the add.
 */
export async function insertBlock(
  channel: string,
  index: number,
  entry: BlockPayload,
): Promise<void> {
  const before = new Set(
    (state.value?.channels.find((c) => c.id === channel)?.blocks ?? []).map((b) => b.id),
  );
  await mutate("addBlock", { channel, type: entry.type, name: entry.name });
  const blocks = state.value?.channels.find((c) => c.id === channel)?.blocks ?? [];
  const added = blocks.find((b) => !before.has(b.id));
  if (!added) return;
  const to = Math.max(0, Math.min(index, blocks.length - 1));
  if (to !== blocks.indexOf(added)) {
    await mutate("moveBlock", { channel, block: added.id, to });
  }
}

export const removeBlock = (channel: string, block: string) =>
  mutate("removeBlock", { channel, block });

export const moveBlock = (channel: string, block: string, to: number) =>
  mutate("moveBlock", { channel, block, to });

export const setSources = (channel: string, sources: string[]) =>
  mutate("setSources", { channel, sources });

// --- Cross-channel routings -------------------------------------------------
// A routing is a tap from one block to a block in another channel. Every
// command returns the full snapshot, so the list on screen is always the
// engine's, never a local guess.

export const createRouting = (source: RoutingEndpoint, dest: RoutingEndpoint) =>
  mutate("createRouting", { source, dest });

export const removeRouting = (id: string) => mutate("removeRouting", { id });

export const setEnabled = (id: string, enabled: boolean) =>
  mutate("setEnabled", { id, enabled });

export const setGain = (id: string, gain: number) => mutate("setGain", { id, gain });

export const listRoutings = () => mutate("listRoutings");

/** Select a routing line (or `null` to clear). Never sent to the engine. */
export const selectRouting = (id: string | null) => {
  selectedRouting.value = id;
};

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
