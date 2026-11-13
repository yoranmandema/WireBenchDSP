import { useState } from "preact/hooks";
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

const DRAG_MIME = "application/x-pedal-block";

/** One channel row: header above a horizontally scrolling strip of blocks. */
function ChannelRow({ channel }: { channel: import("../protocol").Channel }) {
  const s = state.value!;
  const [dragOver, setDragOver] = useState<number | null>(null);

  const drop = (to: number) => {
    const raw = (window as unknown as { __drag?: string }).__drag;
    setDragOver(null);
    if (!raw) return;
    const from = channel.blocks.findIndex((b) => b.id === raw);
    if (from >= 0 && from !== to) void moveBlock(channel.id, raw, to);
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

      <div class="strip">
        <div class="io">In</div>
        {channel.blocks.map((b, i) => (
          <div
            key={b.id}
            class={`block-card${b.bypass ? " bypass" : ""}${dragOver === i ? " over" : ""}`}
            draggable
            onDragStart={(e) => {
              (e as unknown as DragEvent).dataTransfer?.setData(DRAG_MIME, b.id);
              (window as unknown as { __drag?: string }).__drag = b.id;
            }}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(i);
            }}
            onDrop={() => drop(i)}
            onClick={() => select(channel.id, b.id)}
            title={`${b.name} — drag to reorder`}
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
        ))}
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
      <p class="hint">Drag a block sideways to reorder it. Click a block to edit it below.</p>
    </div>
  );
}
