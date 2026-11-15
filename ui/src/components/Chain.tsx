import { Fragment } from "preact";
import { useRef, useState } from "preact/hooks";
import { BlockIcon, CloseIcon, PlusIcon, PowerIcon } from "./Icon";
import { catStyle } from "../theme";
import { decodeBlockPayload, isBlockDrag } from "../dnd";
import {
  insertBlock,
  moveBlock,
  openBlockPicker,
  removeBlock,
  select,
  selected,
  setBypass,
  setMasterVolume,
  setSources,
  state,
} from "../state";

/** Live state of a pointer-driven block drag. */
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

/** One channel row: header above a horizontally scrolling strip of blocks. */
function ChannelRow({ channel }: { channel: import("../protocol").Channel }) {
  const s = state.value!;
  const [drag, setDrag] = useState<Drag | null>(null);
  const [drop, setDrop] = useState<Drop | null>(null);
  const strip = useRef<HTMLDivElement>(null);
  // Pending press (not yet past the drag threshold) and the click/drag verdict.
  const pending = useRef<{ id: string; from: number; x: number; y: number } | null>(null);
  const dragged = useRef(false);

  /** Slot under the pointer: the first card whose midpoint is to the right. */
  function slotAt(clientX: number): number {
    const els = Array.from(strip.current?.querySelectorAll(".block-card") ?? []) as HTMLElement[];
    for (const el of els) {
      const r = el.getBoundingClientRect();
      if (clientX < r.left + r.width / 2) return Number(el.dataset.slot);
    }
    return channel.blocks.length;
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
    const payload = decodeBlockPayload(e.dataTransfer);
    // Not a block, or a type the engine does not offer: nothing to insert.
    if (!payload || !s.registry[payload.type]) return;
    void insertBlock(channel.id, slot, payload);
  };

  const down = (e: PointerEvent, blockId: string, index: number) => {
    if (e.button !== 0) return;
    // The bypass/remove buttons are controls, not drag handles.
    if ((e.target as Element).closest("button")) return;
    dragged.current = false;
    pending.current = { id: blockId, from: index, x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  };

  const move = (e: PointerEvent) => {
    if (drag) {
      const insert = slotAt(e.clientX);
      if (insert === drag.insert && e.clientX === drag.x && e.clientY === drag.y) return;
      setDrag({ ...drag, insert, x: e.clientX, y: e.clientY });
      return;
    }
    const p = pending.current;
    if (!p) return;
    const dx = Math.abs(e.clientX - p.x);
    const dy = Math.abs(e.clientY - p.y);
    if (dx < 4 && dy < 4) return; // still a click
    dragged.current = true;
    pending.current = null;
    setDrag({
      id: p.id,
      from: p.from,
      // The move that crosses the threshold is already a target position.
      insert: slotAt(e.clientX),
      x: e.clientX,
      y: e.clientY,
    });
  };

  const up = () => {
    const d = drag;
    pending.current = null;
    setDrag(null);
    if (!d || !dragged.current) return; // plain click: onClick selects
    // `moveBlock` indexes the array *after* the removal, so shift the target
    // down one slot when the block leaves a position left of its landing slot.
    const to = Math.min(d.insert > d.from ? d.insert - 1 : d.insert, channel.blocks.length - 1);
    if (to !== d.from) void moveBlock(channel.id, d.id, to);
  };

  const cancel = () => {
    pending.current = null;
    setDrag(null);
  };

  // The dragged block's spec, for the ghost's icon and colour.
  const dragBlock = drag ? channel.blocks.find((b) => b.id === drag.id) : undefined;
  const dragSpec = dragBlock ? s.registry[dragBlock.type] : undefined;

  return (
    <div
      class={`channel${drop ? " drop-target-active" : ""}`}
      onDragEnter={dragEnter}
      onDragOver={dragOver}
      onDragLeave={dragLeave}
      onDrop={dropFromBrowser}
    >
      <div class="channel-head">
        <span class="channel-name">{channel.name}</span>
        <input
          type="range"
          min={-60}
          max={12}
          step={0.1}
          value={channel.volume}
          title={`Volume ${channel.volume.toFixed(1)} dB`}
          onInput={(e) => void setMasterVolume(channel.id, Number((e.target as HTMLInputElement).value))}
        />
        <label class="mini">
          <input
            type="checkbox"
            checked={channel.sources.includes("<in>")}
            onChange={(e) =>
              void setSources(
                channel.id,
                (e.target as HTMLInputElement).checked
                  ? [...new Set([...channel.sources, "<in>"])]
                  : channel.sources.filter((x) => x !== "<in>"),
              )
            }
          />
          In
        </label>
      </div>

      <div class="strip" ref={strip}>
        <div class="io in">In</div>
        {channel.blocks.map((b, i) => {
          const spec = s.registry[b.type];
          const isSel = selected.value?.block === b.id;
          return (
            <Fragment key={b.id}>
              {/* Always-mounted slot so the cards themselves are never added or
                  removed mid-drag (that would break pointer capture). */}
              <div
                class={`block-placeholder${drag && drag.insert === i && drag.insert !== drag.from ? " active" : ""}`}
              />
              <div
                class={[
                  "block-card",
                  b.bypass ? "bypass" : "",
                  isSel ? "selected" : "",
                  drag?.id === b.id ? "dragging" : "",
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
                onPointerDown={(e) => down(e, b.id, i)}
                onPointerMove={move}
                onPointerUp={up}
                onPointerCancel={cancel}
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
                onClick={() => {
                  // A drag ends with a synthetic click on the same card; swallow
                  // that one, but never swallow clicks after it.
                  if (dragged.current) {
                    dragged.current = false;
                    return;
                  }
                  select(channel.id, b.id);
                }}
                title={`${b.name} (${b.type}) — drag to reorder, click to edit`}
              >
                <button
                  class={`icon-btn bypass${b.bypass ? " on" : ""}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    void setBypass(channel.id, b.id, !b.bypass);
                  }}
                  title={b.bypass ? "Unbypass" : "Bypass"}
                >
                  <PowerIcon size={10} />
                </button>
                <button
                  class="icon-btn remove"
                  onClick={(e) => {
                    e.stopPropagation();
                    void removeBlock(channel.id, b.id);
                  }}
                  title="Remove block"
                >
                  <CloseIcon size={9} />
                </button>
                <span class="block-icon">
                  {spec && <BlockIcon type={b.type} category={spec.category} size={26} />}
                </span>
                <span class="block-name">{b.name}</span>
              </div>
            </Fragment>
          );
        })}
        <div
          class={`block-placeholder${drag && drag.insert === channel.blocks.length && drag.insert !== drag.from ? " active" : ""}`}
        />
        {drop && <div class="drop-indicator" style={{ left: `${drop.x}px` }} />}
        {channel.blocks.length === 0 && (
          <div class="strip-empty">Empty channel — press + to search for a block</div>
        )}
        <button
          class="add-block"
          onClick={() => openBlockPicker(channel.id)}
          title="Search and add a block"
        >
          <PlusIcon size={16} />
        </button>
        <div class="io out">Out</div>
      </div>

      {/* Ghost of the dragged card, following the pointer. */}
      {drag && (
        <div
          class="drag-ghost"
          style={{
            left: `${drag.x}px`,
            top: `${drag.y}px`,
            ...(dragSpec ? catStyle(dragSpec.category) : null),
          }}
        >
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
  if (!s) return null;
  return (
    <div class="chain-panel">
      <div class="chain-label">Signal chain</div>
      <div class="chain-scroll">
        {s.channels.map((c) => (
          <ChannelRow key={c.id} channel={c} />
        ))}
      </div>
      <p class="hint">Drag to reorder · click a block to edit it below.</p>
    </div>
  );
}
