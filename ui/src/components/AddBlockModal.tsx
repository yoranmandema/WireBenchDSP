import { useEffect, useRef } from "preact/hooks";
import { addBlock, addBlockFor, blockQuery, state } from "../state";
import { BlockList, groupBlocks, type BlockEntry } from "./BlockList";
import { emptyNote, modalBackdrop, modalFoot, modalHead, modalShell, modalTitle, navBtn, textField } from "../ui";

/**
 * Put focus back on the channel's "+" button — the control that opened the
 * picker. The button carries `data-add` with its channel id, so this finds the
 * replacement button too, after an empty channel's row remounts on the first
 * block. Without it, closing the modal drops focus on the page body.
 */
function focusAddButton(channel: string): void {
  // A slow engine reply can land after the user has already opened the picker
  // on another channel; never steal focus from that newer picker.
  if (addBlockFor.value) return;
  document.querySelector<HTMLElement>(`[data-add="${channel}"]`)?.focus();
}

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
      if (e.key === "Escape") {
        addBlockFor.value = null;
        focusAddButton(channelId);
      }
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
  const channelName = s.channels.find((c) => c.id === channelId)?.name ?? "channel";

  /** Close the picker, returning focus to the channel's "+" button. */
  const close = () => {
    addBlockFor.value = null;
    focusAddButton(channelId);
  };

  const pick = async (b: BlockEntry) => {
    // A double-click can deliver its second click before the modal has
    // unmounted; only the first one may add a block.
    if (addBlockFor.value !== channelId) return;
    addBlockFor.value = null;
    await addBlock(channelId, b.type, b.name);
    // Adding the first block to an empty channel swaps that channel's row
    // component, which remounts the "+" button; re-query rather than trust
    // whatever element held focus before.
    focusAddButton(channelId);
  };

  return (
    <div class={`${modalBackdrop} modalBackdrop modal-backdrop`} onClick={close}>
      <div class={`${modalShell} modalShell modal picker w-[520px]`} onClick={(e) => e.stopPropagation()}>
        <header class={`${modalHead} modalHead modal-head`}>
          <h2 class={`${modalTitle} modalTitle modal-title`}>Add block</h2>
          <span class="picker-channel px-2 py-0.5 text-[10px] tracking-[0.8px] uppercase text-muted border border-line rounded-full whitespace-nowrap" title={`Adding to ${channelName}`}>{channelName}</span>
          <button class={`${navBtn} navBtn nav`} onClick={close} title="Close">×</button>
        </header>

        <input class={`${textField} textField picker-search mx-3.5 my-3 px-2.5 py-2`} ref={inputRef} type="search" placeholder="Search blocks…" value={blockQuery.value} title="Filter the block list by name or type" onInput={(e) => (blockQuery.value = (e.target as HTMLInputElement).value)} onKeyDown={(e) => { if (e.key !== "Enter") return; e.preventDefault(); const first = groups[0]?.blocks[0]; if (first) pick(first); }} />

        <div class="picker-body flex-1 overflow-y-auto px-2.5 pb-3">
          {groups.length === 0 ? <div class={`${emptyNote} editor-empty emptyNote`}>No blocks match “{blockQuery.value.trim()}”.</div> : <BlockList groups={groups} onSelect={pick} />}
        </div>

        <footer class={`${modalFoot} modalFoot modal-foot picker-foot justify-between`}>
          <span class="picker-meta text-[10px] text-muted">{count} block{count === 1 ? "" : "s"}</span>
          <span class="picker-meta text-[10px] text-muted">Enter adds the first · Esc closes</span>
        </footer>
      </div>
    </div>
  );
}