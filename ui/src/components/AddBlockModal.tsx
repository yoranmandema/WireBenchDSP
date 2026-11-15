import { useEffect, useRef } from "preact/hooks";
import {
  addBlock,
  addBlockFor,
  blockQuery,
  state,
} from "../state";
import { BlockList, groupBlocks, type BlockEntry } from "./BlockList";

/**
 * The add-block picker: press + on a channel and this modal opens with the
 * focus in a search field. Typing filters the catalogue (the rack's own list),
 * clicking a row adds that block to the channel the picker was opened from and
 * closes. Enter adds the first match; Escape or the backdrop closes.
 */
export function AddBlockModal() {
  const s = state.value;
  const channelId = addBlockFor.value;
  const inputRef = useRef<HTMLInputElement>(null);

  // Autofocus the search field whenever the picker opens, and make Escape
  // dismiss it from anywhere (the field is focused, but a global listener also
  // catches the key when focus has moved into the list).
  useEffect(() => {
    if (!channelId) return;
    const t = setTimeout(() => inputRef.current?.focus(), 0);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") addBlockFor.value = null;
    };
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(t);
      window.removeEventListener("keydown", onKey);
    };
  }, [channelId]);

  if (!channelId || !s) return null;

  const groups = groupBlocks(s.registry, blockQuery.value);
  const count = groups.reduce((n, g) => n + g.blocks.length, 0);
  const channelName =
    s.channels.find((c) => c.id === channelId)?.name ?? "channel";

  const pick = (b: BlockEntry) => {
    void addBlock(channelId, b.type, b.name);
    addBlockFor.value = null;
  };
  return (
    <div class="modal-backdrop" onClick={() => (addBlockFor.value = null)}>
      <div class="modal picker" onClick={(e) => e.stopPropagation()}>
        <header class="modal-head">
          <h2>Add block</h2>
          <span class="picker-channel" title={`Adding to ${channelName}`}>
            {channelName}
          </span>
          <button
            class="nav"
            onClick={() => (addBlockFor.value = null)}
            title="Close"
          >
            ×
          </button>
        </header>

        <input
          class="picker-search"
          ref={inputRef}
          type="search"
          placeholder="Search blocks…"
          value={blockQuery.value}
          title="Filter the block list by name or type"
          onInput={(e) =>
            (blockQuery.value = (e.target as HTMLInputElement).value)
          }
          onKeyDown={(e) => {
            if (e.key !== "Enter") return;
            e.preventDefault();
            const first = groups[0]?.blocks[0];
            if (first) pick(first);
          }}
        />

        <div class="picker-body">
          {groups.length === 0 ? (
            <div class="editor-empty">
              No blocks match “{blockQuery.value.trim()}”.
            </div>
          ) : (
            <BlockList groups={groups} onSelect={pick} />
          )}
        </div>

        <footer class="modal-foot picker-foot">
          <span class="picker-meta">
            {count} block{count === 1 ? "" : "s"}
          </span>
          <span class="picker-meta">Enter adds the first · Esc closes</span>
        </footer>
      </div>
    </div>
  );
}
