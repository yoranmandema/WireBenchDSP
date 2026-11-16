import { Fragment } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";
import { BlockIcon, CloseIcon, PlusIcon, PowerIcon } from "./Icon";
import { catStyle } from "../theme";
import { decodeBlockPayload, isBlockDrag, rejectDrop, type BlockPayload } from "../dnd";
import { registerTouchChannel, registerTouchTarget, useTouchDropRouter, useTouchDrag } from "../touch";
import {
  appendSelectedBlock,
  insertBlock,
  moveBlock,
  openBlockPicker,
  removeBlock,
  select,
  selected,
  selectedBlockType,
  setBypass,
  setMasterVolume,
  setSources,
  state,
} from "../state";

/** Live state of a pointer-driven block drag (a card reorder). */
interface Drag {
  /** Block being dragged. */
  id: string;
  /** Index it started at, in the channel's block array. */
  from: number;
  /** Slot it would land in, counting the gaps between cards (0..len). */
  insert: number;
  /** Pointer position, for the floating ghost. */
  x: number;
  y: number;
}

/** Live state of a block dragged from the browser hovering this channel. */
interface Drop {
  /** Slot the insertion line sits at (0..len). */
  slot: number;
  /** Content-space x of that slot, for the indicator. */
  x: number;
}

/** Warn about a refused drop. One line, never a thrown error. */
function warnDrop(reason: string): void {
  console.warn(`[chain] ignoring drop: ${reason}`);
}

/**
 * Read and validate a drop's payload. Returns the payload, or null after
 * logging why it was refused.
 *
 * The two refusal cases: no usable payload at all, and a payload whose type id
 * the engine does not offer.
 */
function payloadFromDrop(
  dt: DataTransfer | null,
  registry: Record<string, unknown>,
): BlockPayload | null {
  const payload = decodeBlockPayload(dt);
  const rejection = rejectDrop(payload, registry);
  if (rejection === "no-payload") warnDrop("no block payload on the data transfer");
  else if (rejection === "bad-type") warnDrop("payload has no block type id");
  else if (rejection === "unknown-type") warnDrop(`unknown block type "${payload?.type}"`);
  return rejection ? null : payload;
}

/** One channel row: header above a horizontally scrolling strip of blocks. */
// `min-w-0` on the channel (and on its `.strip`) is load-bearing, not
// defensive: the strip is an `overflow-x-auto` row of fixed-width cards, so its
// min-content width is the entire row laid out end to end. A grid item
// defaults to `min-width: auto`, which hands that min-content width to the
// grid track — the channel then grows past the viewport and the whole chain
// panel scrolls horizontally instead of the strip scrolling on its own. Both
// the channel and the strip therefore opt out of the automatic minimum.
function ChannelRow({ channel }: { channel: import("../protocol").Channel }) {
  // An empty channel is a fixed slot that happens to hold nothing, not a missing
  // one. So it still has to be *there*: header, signal path, the empty hint and
  // the "+" button all stay, and the slot keeps its place in the layout. What
  // it must not do is pay for any of the per-block machinery.
  //
  // Everything below the header is therefore split on this one check. The
  // populated branch is the behaviour this component always had; the empty
  // branch renders the same shell with no cards and no card pointer handlers to
  // bind (it keeps the drop handlers, because an empty channel is still where a
  // dragged block lands, and it registers with the touch router, because a long
  // press from the rack can land there too). Dropping a block in flips
  // `blocks.length`, the branch swaps on the next snapshot, and the channel
  // resumes normal processing — no reload, and the row itself is not remounted.
  if (channel.blocks.length === 0) return <EmptyChannel channel={channel} />;
  return <PopulatedChannel channel={channel} />;
}

/**
 * The slot under the pointer: the first card whose midpoint is to the right.
 * Shared by both branches — a populated channel has cards to measure, an empty
 * one has none and so always answers 0, which is the only slot it has.
 */
function slotAtFor(strip: HTMLDivElement | null, clientX: number, count: number): number {
  const els = Array.from(strip?.querySelectorAll(".block-card") ?? []) as HTMLElement[];
  for (const el of els) {
    const r = el.getBoundingClientRect();
    if (clientX < r.left + r.width / 2) return Number(el.dataset.slot);
  }
  return count;
}

/**
 * The shell of a channel with nothing in it.
 *
 * Deliberately a separate component: it has no drag state and no card pointer
 * handlers, so Preact mounts a handful of elements and nothing more. It is not
 * `null` and not `display: none` — the slot stays visible and keeps its height,
 * which is what makes an empty channel "existent" in the layout.
 */
function EmptyChannel({ channel }: { channel: import("../protocol").Channel }) {
  const s = state.value!;
  const strip = useRef<HTMLDivElement>(null);
  const [dropActive, setDropActive] = useState(false);

  // The touch router resolves the channel under the finger and calls these.
  useEffect(
    () =>
      registerTouchTarget(channel.id, {
        preview: (slot) => setDropActive(slot !== null),
        commit: (payload, slot) => {
          setDropActive(false);
          void insertBlock(channel.id, slot, payload);
        },
      }),
    [channel.id],
  );

  // Keep the router's geometry current: it measures `.block-card` children.
  useEffect(() => registerTouchChannel(channel.id, strip.current), [channel.id]);

  const dragEnter = (e: DragEvent) => {
    const dt = e.dataTransfer;
    if (!dt || !isBlockDrag(dt)) return;
    e.preventDefault();
    dt.dropEffect = "copy";
    setDropActive(true);
  };

  const dragOver = (e: DragEvent) => {
    const dt = e.dataTransfer;
    if (!dt || !isBlockDrag(dt)) return;
    // Required, or the browser will refuse the drop.
    e.preventDefault();
    dt.dropEffect = "copy";
    setDropActive(true);
  };

  const dragLeave = (e: DragEvent) => {
    const next = e.relatedTarget as Node | null;
    if (next && (e.currentTarget as HTMLElement).contains(next)) return;
    setDropActive(false);
  };

  const drop = (e: DragEvent) => {
    e.preventDefault();
    setDropActive(false);
    const payload = payloadFromDrop(e.dataTransfer, s.registry);
    if (!payload) return;
    void insertBlock(channel.id, slotAtFor(strip.current, e.clientX, 0), payload);
  };

  return (
    <div
      class={`channel is-empty min-w-0 flex flex-col rounded-card border ${dropActive ? "drop-target-active border-amber-dim bg-[rgba(232,163,61,0.05)] shadow-[0_0_0_1px_var(--color-amber-dim),0_0_22px_rgba(232,163,61,0.22)]" : "border-line-soft bg-[rgba(255,255,255,0.012)]"} p-2.5 transition-[border-color,box-shadow,background-color] duration-120`}
      data-channel-id={channel.id}
      onDragEnter={dragEnter}
      onDragOver={dragOver}
      onDragLeave={dragLeave}
      onDrop={drop}
    >
      <ChannelHead channel={channel} />
      {/* `min-w-0` is not cosmetic: the strip scrolls its own overflow, so its
          min-content width (every card laid out in a row) must not be allowed
          to widen the grid track. Without it the channel grows past the
          viewport and the whole chain panel scrolls instead. */}
      <div class="strip flex min-h-[102px] min-w-0 items-center gap-2 overflow-x-auto px-2 py-[18px]" ref={strip}>
        <div class="io in flex-none text-xs text-muted px-2 py-1.5 border border-line-soft rounded-key bg-[#1a1b21]">In</div>
        <div class="strip-empty flex-1 text-center text-sm text-muted">Empty channel — press + to search for a block</div>
        <AddBlockButton channel={channel} />
        <div class="io out flex-none text-xs text-muted px-2 py-1.5 border border-line-soft rounded-key bg-[#1a1b21]">Out</div>
      </div>
    </div>
  );
}

/** The channel's "+" button and the keyboard-insert button. */
function AddBlockButton({ channel }: { channel: import("../protocol").Channel }) {
  const chosen = selectedBlockType.value;
  return (
    <>
      <button class="add-block flex-none size-8.5 rounded-full border-[1px] border-dashed border-[#3f424d] text-muted text-[17px] leading-none hover:text-text hover:border-amber" onClick={() => openBlockPicker(channel.id)} title="Search and add a block">
        <PlusIcon size={16} />
      </button>
      {/* Keyboard alternative to dragging: the block type selected in the rack
          (Enter/Space on a row) is inserted at the end of this channel. */}
      <button
        class={`add-selected flex-none size-8 rounded-full border text-[9px] font-semibold leading-none transition-[border-color,background-color] ${chosen ? "border-amber-dim text-amber bg-[rgba(232,163,61,0.1)] hover:bg-[rgba(232,163,61,0.18)]" : "border-line-soft text-muted bg-[rgba(255,255,255,0.012)]"}`}
        disabled={!chosen}
        onClick={() => { if (chosen) void appendSelectedBlock(channel.id, chosen); }}
        title={chosen ? `Add ${chosen.name} to ${channel.name} at the end` : `Select a block in the rack (Enter) to add it to ${channel.name}`}
        aria-label={chosen ? `Add ${chosen.name} to ${channel.name}` : `No block selected for ${channel.name}`}
      >
        {chosen ? "Add" : "–"}
      </button>
    </>
  );
}

/** The always-present header: name, volume, and the input-source toggle. */
function ChannelHead({ channel }: { channel: import("../protocol").Channel }) {
  return (
    <div class="channel-head flex items-center gap-2.5 mb-2">
      <span class="channel-name text-[11px] font-semibold tracking-[0.4px]">{channel.name}</span>
      <input type="range" min={-60} max={12} step={0.1} value={channel.volume} title={`Volume ${channel.volume.toFixed(1)} dB`} onInput={(e) => void setMasterVolume(channel.id, Number((e.target as HTMLInputElement).value))} class="w-[130px] accent-amber" />
      <label class="mini flex items-center gap-1 text-xs text-muted">
        <input type="checkbox" checked={channel.sources.includes("<in>")} onChange={(e) => void setSources(channel.id, (e.target as HTMLInputElement).checked ? [...new Set([...channel.sources, "<in>"])] : channel.sources.filter((x) => x !== "<in>"))} />
        In
      </label>
    </div>
  );
}

/** A channel holding at least one block: the drag-and-drop strip. */
function PopulatedChannel({ channel }: { channel: import("../protocol").Channel }) {
  const s = state.value!;
  const [drag, setDrag] = useState<Drag | null>(null);
  const [drop, setDrop] = useState<Drop | null>(null);
  const strip = useRef<HTMLDivElement>(null);

  /** Slot under the pointer: the first card whose midpoint is to the right. */
  function slotAt(clientX: number): number {
    return slotAtFor(strip.current, clientX, channel.blocks.length);
  }

  /** Content-space x of the insertion boundary before `slot`. */
  function boundaryX(slot: number): number {
    const el = strip.current;
    if (!el) return 0;
    const rect = el.getBoundingClientRect();
    const cards = Array.from(el.querySelectorAll(".block-card")) as HTMLElement[];
    if (cards.length === 0) {
      const io = el.querySelector(".io.in") as HTMLElement | null;
      const r = io?.getBoundingClientRect();
      return r ? r.right - rect.left + el.scrollLeft : 0;
    }
    if (slot >= cards.length) {
      const last = cards[cards.length - 1].getBoundingClientRect();
      return last.right - rect.left + el.scrollLeft;
    }
    return cards[slot].getBoundingClientRect().left - rect.left + el.scrollLeft;
  }

  /** Remember the slot under the pointer and paint the insertion line. */
  function trackDrop(e: DragEvent) {
    const slot = slotAt(e.clientX);
    setDrop((d) => (d && d.slot === slot ? d : { slot, x: boundaryX(slot) }));
  }

  const dragEnter = (e: DragEvent) => {
    const dt = e.dataTransfer;
    if (!dt || !isBlockDrag(dt)) return;
    e.preventDefault();
    dt.dropEffect = "copy";
    trackDrop(e);
  };

  const dragOver = (e: DragEvent) => {
    const dt = e.dataTransfer;
    if (!dt || !isBlockDrag(dt)) return;
    // Required, or the browser will refuse the drop.
    e.preventDefault();
    dt.dropEffect = "copy";
    trackDrop(e);
  };

  const dragLeave = (e: DragEvent) => {
    // `dragleave` also fires when the pointer crosses into a child; only a
    // leave of the channel itself clears the highlight.
    const next = e.relatedTarget as Node | null;
    if (next && (e.currentTarget as HTMLElement).contains(next)) return;
    setDrop(null);
  };

  const dropFromBrowser = (e: DragEvent) => {
    e.preventDefault();
    const slot = drop?.slot ?? slotAt(e.clientX);
    setDrop(null);
    const payload = payloadFromDrop(e.dataTransfer, s.registry);
    if (!payload) return;
    void insertBlock(channel.id, slot, payload);
  };

  // --- Pointer reorder -----------------------------------------------------
  // A card drag (mouse or touch) reorders. `dragCardId` is set by `cardDown`
  // before the gesture's `onStart`, so the start handler knows which card.
  // `useTouchDrag` owns the long-press/drag-threshold timing.

  const dragCardId = useRef<string | null>(null);
  const cardPress = useTouchDrag({
    onStart: (x, y) => {
      const id = dragCardId.current;
      if (!id) return;
      const from = channel.blocks.findIndex((b) => b.id === id);
      if (from < 0) return;
      setDrag({ id, from, insert: slotAt(x), x, y });
    },
    onMove: (x, y) => {
      setDrag((d) => {
        if (!d) return d;
        const insert = slotAt(x);
        if (insert === d.insert && x === d.x && y === d.y) return d;
        return { ...d, insert, x, y };
      });
    },
    onEnd: (dragged) => {
      const d = drag;
      setDrag(null);
      if (!dragged || !d) return;
      // `moveBlock` indexes the array *after* the removal, so shift the target
      // down one slot when the block leaves a position left of its landing slot.
      const to = Math.min(d.insert > d.from ? d.insert - 1 : d.insert, channel.blocks.length - 1);
      if (to !== d.from) void moveBlock(channel.id, d.id, to);
    },
  });

  const cardDown = (e: PointerEvent, blockId: string) => {
    // The bypass/remove buttons are controls, not drag handles.
    if ((e.target as Element).closest("button")) return;
    dragCardId.current = blockId;
    cardPress.onPointerDown(e);
  };

  // --- Touch drag from the rack -------------------------------------------
  // `Chain` routes a rack long-press to the channel under the finger; this
  // channel previews the insertion line and inserts on release.

  useEffect(
    () =>
      registerTouchTarget(channel.id, {
        preview: (slot) => {
          if (slot === null) {
            setDrop(null);
            return;
          }
          setDrop((d) => (d && d.slot === slot ? d : { slot, x: boundaryX(slot) }));
        },
        commit: (payload, slot) => {
          setDrop(null);
          void insertBlock(channel.id, slot, payload);
        },
      }),
    // `boundaryX` closes over the current strip and block count.
    [channel.id, channel.blocks.length],
  );

  // Keep the router's geometry current: it measures `.block-card` children.
  useEffect(() => registerTouchChannel(channel.id, strip.current), [channel.id]);

  // The dragged block's spec, for the ghost's icon and colour.
  const dragBlock = drag ? channel.blocks.find((b) => b.id === drag.id) : undefined;
  const dragSpec = dragBlock ? s.registry[dragBlock.type] : undefined;

  return (
    <div
      class={`channel min-w-0 flex flex-col rounded-card border ${drop ? "drop-target-active border-amber-dim bg-[rgba(232,163,61,0.05)] shadow-[0_0_0_1px_var(--color-amber-dim),0_0_22px_rgba(232,163,61,0.22)]" : "border-line-soft bg-[rgba(255,255,255,0.012)]"} p-2.5 transition-[border-color,box-shadow,background-color] duration-120`}
      data-channel-id={channel.id}
      onDragEnter={dragEnter}
      onDragOver={dragOver}
      onDragLeave={dragLeave}
      onDrop={dropFromBrowser}
    >
      <ChannelHead channel={channel} />

      <div class="strip relative flex min-h-[102px] min-w-0 items-center gap-2 overflow-x-auto px-2 py-[18px]" ref={strip}>
        <div class="io in flex-none text-xs text-muted px-2 py-1.5 border border-line-soft rounded-key bg-[#1a1b21]">In</div>
        {channel.blocks.map((b, i) => {
          const spec = s.registry[b.type];
          const isSel = selected.value?.block === b.id;
          return (
            <Fragment key={b.id}>
              {/* Always-mounted slot so the cards themselves are never added or
                  removed mid-drag (that would break pointer capture). */}
              <div class={`block-placeholder flex-none h-[66px] rounded-lg border-[1px] border-dashed border-transparent overflow-hidden transition-[width,margin-right,border-color,background-color] duration-110 ${drag && drag.insert === i && drag.insert !== drag.from ? "w-[116px] mr-0 border-amber-dim bg-[rgba(232,163,61,0.08)]" : "w-0 mr-[-8px]"}`} />
              <div
                class={[
                  "block-card cat-scope flex-none w-[116px] h-[66px] rounded-lg border border-[#353843] border-l-3 bg-linear-to-r from-cat-soft to-[#1b1c22] shadow-card grid grid-rows-[auto_1fr_auto] items-center justify-items-center gap-0.5 px-2 py-1 cursor-grab select-none relative touch-none transition-[border-color,box-shadow,transform] duration-120 hover:border-[#4a4d59] before:content-[''] before:absolute before:-left-1 before:top-1/2 before:size-[5px] before:-translate-y-1/2 before:rounded-full before:bg-cat before:shadow-[0_0_5px_var(--cat)] after:content-[''] after:absolute after:-right-1 after:top-1/2 after:size-[5px] after:-translate-y-1/2 after:rounded-full after:bg-cat after:shadow-[0_0_5px_var(--cat)]",
                  b.bypass ? "bypass opacity-45" : "",
                  isSel ? "selected border-cat shadow-[inset_0_0_0_1px_var(--cat)] -translate-y-0.5" : "",
                  drag?.id === b.id ? "dragging border-dashed border-[#4a4d59] opacity-35" : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                data-slot={i}
                style={{
                  ...(spec ? catStyle(spec.category) : null),
                  ...(drag?.id === b.id ? { opacity: 0.35 } : null),
                }}
                // Keyboard selection: the card is a button in all but name, so
                // Tab reaches it and Enter/Space select it.
                tabIndex={0}
                {...cardPress}
                onPointerDown={(e) => cardDown(e, b.id)}
                onKeyDown={(e) => {
                  // Enter/Space on the card itself selects it. If the key was
                  // pressed on an inner button (bypass/remove), let the button
                  // handle it natively instead of also selecting the card.
                  if (e.target !== e.currentTarget) return;
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    select(channel.id, b.id);
                  }
                }}
                onClick={() => select(channel.id, b.id)}
                title={`${b.name} (${b.type}) — drag to reorder, click to edit`}
              >
                <button class={`icon-btn bypass absolute top-1 left-1 size-4 rounded-full grid place-items-center text-[9px] leading-none bg-[#26282f] text-muted border border-line-soft ${b.bypass ? "bg-amber text-amber-ink border-amber-edge" : ""}`} onClick={(e) => { e.stopPropagation(); void setBypass(channel.id, b.id, !b.bypass); }} title={b.bypass ? "Unbypass" : "Bypass"}>
                  <PowerIcon size={10} />
                </button>
                <button class="icon-btn remove absolute top-1 right-1 size-4 rounded-full grid place-items-center text-[9px] leading-none bg-[#26282f] text-muted border border-line-soft hover:text-[#ff8a7a] hover:border-[#5a3535]" onClick={(e) => { e.stopPropagation(); void removeBlock(channel.id, b.id); }} title="Remove block">
                  <CloseIcon size={9} />
                </button>
                <span class="block-icon grid place-items-center h-[14px] text-cat">
                  {spec && <BlockIcon type={b.type} category={spec.category} size={26} />}
                </span>
                <span class={`block-name text-[11px] font-semibold text-center leading-[1.15] ${b.bypass ? "line-through" : ""}`}>{b.name}</span>
              </div>
            </Fragment>
          );
        })}
        <div class={`block-placeholder flex-none h-[66px] rounded-lg border-[1px] border-dashed border-transparent overflow-hidden transition-[width,margin-right,border-color,background-color] duration-110 ${drag && drag.insert === channel.blocks.length && drag.insert !== drag.from ? "w-[116px] mr-0 border-amber-dim bg-[rgba(232,163,61,0.08)]" : "w-0 mr-[-8px]"}`} />
        {drop && <div class="drop-indicator absolute top-[14px] bottom-[14px] w-[2px] -ml-[1px] rounded rounded-[1px] bg-amber shadow-[0_0_8px_rgba(232,163,61,0.8)] z-20 pointer-events-none" style={{ left: `${drop.x}px` }} />}
        <AddBlockButton channel={channel} />
        <div class="io out flex-none text-xs text-muted px-2 py-1.5 border border-line-soft rounded-key bg-[#1a1b21]">Out</div>
      </div>

      {/* Ghost of the dragged card, following the pointer. */}
      {drag && (
        <div class="drag-ghost fixed z-40 transform -translate-x-1/2 -translate-y-1/2 scale-[1.04] px-3.5 py-2 rounded-lg bg-linear-to-b from-[#3a3e49] to-[#23252c] border border-amber shadow-drag text-text text-[11px] font-semibold whitespace-nowrap pointer-events-none" style={{ left: `${drag.x}px`, top: `${drag.y}px`, ...(dragSpec ? catStyle(dragSpec.category) : null) }}>
          {dragSpec && <BlockIcon type={dragBlock!.type} category={dragSpec.category} size={14} />}
          {dragBlock?.name}
        </div>
      )}
    </div>
  );
}

/** The centre panel: one row per channel, blocks running left to right. */
export function Chain() {
  const s = state.value;
  // One window-level router for the whole chain: it resolves the channel under
  // the finger and forwards preview/commit to that channel.
  useTouchDropRouter();
  if (!s) return null;
  return (
    <div class="chain-panel overflow-auto p-2.5 bg-[radial-gradient(120%_90%_at_50%_0%,#1a1c22_0%,#101116_70%)]">
      <div class="chain-label text-[10px] tracking-[1.4px] text-muted font-semibold mb-2.5">Signal chain</div>
      <div class="chain-scroll grid gap-2.5">
        {s.channels.map((c) => (
          <ChannelRow key={c.id} channel={c} />
        ))}
      </div>
      <p class="hint mt-2.5 text-xs text-muted">Drag to reorder · click a block to edit it below.</p>
    </div>
  );
}
