import { Fragment } from "preact";
import { useRef, useState } from "preact/hooks";
import {
  addBlock,
  moveBlock,
  removeBlock,
  removeChannel,
  select,
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

/** One channel row: header above a horizontally scrolling strip of blocks. */
function ChannelRow({ channel }: { channel: import("../protocol").Channel }) {
  const s = state.value!;
  const [drag, setDrag] = useState<Drag | null>(null);
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

  return (
    <div class="channel">
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
        {s.channels.length > 1 && (
          <button class="mini danger" onClick={() => void removeChannel(channel.id)} title="Remove channel">
            x
          </button>
        )}
      </div>

      <div class="strip" ref={strip}>
        <div class="io">In</div>
        {channel.blocks.map((b, i) => (
          <Fragment key={b.id}>
            {/* Always-mounted slot so the cards themselves are never added or
                removed mid-drag (that would break pointer capture). */}
            <div
              class={`block-placeholder${drag && drag.insert === i && drag.insert !== drag.from ? " active" : ""}`}
            />
            <div
              class={`block-card${b.bypass ? " bypass" : ""}${drag?.id === b.id ? " dragging" : ""}`}
              data-slot={i}
              style={drag?.id === b.id ? { opacity: 0.35 } : undefined}
              onPointerDown={(e) => down(e, b.id, i)}
              onPointerMove={move}
              onPointerUp={up}
              onPointerCancel={cancel}
              onClick={() => {
                // A drag ends with a synthetic click on the same card; swallow
                // that one, but never swallow clicks after it.
                if (dragged.current) {
                  dragged.current = false;
                  return;
                }
                select(channel.id, b.id);
              }}
              title={`${b.name} — drag to reorder, click to edit`}
            >
              <span class="block-name">{b.name}</span>
              <span class="block-type">{b.type}</span>
              <button
                class={`bypass${b.bypass ? " on" : ""}`}
                onClick={(e) => {
                  e.stopPropagation();
                  void setBypass(channel.id, b.id, !b.bypass);
                }}
                title="Bypass"
              >
                B
              </button>
              <button
                class="remove"
                onClick={(e) => {
                  e.stopPropagation();
                  void removeBlock(channel.id, b.id);
                }}
                title="Remove block"
              >
                x
              </button>
            </div>
          </Fragment>
        ))}
        <div
          class={`block-placeholder${drag && drag.insert === channel.blocks.length && drag.insert !== drag.from ? " active" : ""}`}
        />
        {channel.blocks.length === 0 && (
          <div class="strip-empty">Empty channel — add a block from the rack</div>
        )}
        <button
          class="add-block"
          onClick={() => {
            const first = Object.entries(s.registry)[0];
            if (first) void addBlock(channel.id, first[0], first[1].name);
          }}
          title="Add a block"
        >
          +
        </button>
        <div class="io">Out</div>
      </div>

      {/* Ghost of the dragged card, following the pointer. */}
      {drag && (
        <div class="drag-ghost" style={{ left: `${drag.x}px`, top: `${drag.y}px` }}>
          {channel.blocks.find((b) => b.id === drag.id)?.name}
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
      <p class="hint">
        Drag a block sideways to reorder it — the dashed slot shows where it will land. Click a block to edit it below.
      </p>
    </div>
  );
}
