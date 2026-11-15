# Block browser reuse: rack → add-block modal

**Decision: extract.** The shared core was pulled out of the rack as a
presentational component (`BlockList`) with a one-method callback contract
(`onSelect(entry)`). Both the always-visible rack and the add-block modal now
render that same component; no other adaptation was needed.

Status: implemented on `main` (commits `f882339`, `a361168`), verified by the
checks in [Verification](#verification).

## What the "block browser" is

The catalogue of block types the engine offers, grouped by category. It has two
surfaces:

| Surface | File | Host element |
|---|---|---|
| Always-visible rack | `ui/src/components/Rack.tsx` | `<aside class="rack">` |
| Add-block picker modal | `ui/src/components/AddBlockModal.tsx` | `.modal.picker` |

Both call the same renderer:

| Piece | File | Role |
|---|---|---|
| `BlockList` | `ui/src/components/BlockList.tsx` | Pure presentation: grouped sections, one button per block, HTML5 drag sources. |
| `groupBlocks(registry, query)` | same file | Pure grouping/filtering. Owns `CATEGORY_ORDER`. |
| `BlockEntry` / `BlockGroup` | same file | The data shape crossing the boundary. |

## Why extract (and not the alternatives)

| Option | Verdict | Reason |
|---|---|---|
| Reuse directly (rack DOM inside the modal) | rejected | The rack is an `<aside>` with its own column layout and scrolling; those constraints fight the modal shell. |
| Wrap (`<Modal><Rack/></Modal>`) | rejected | Same layout coupling, plus the rack's selection always targets channel 0, so the callback would have to be overridden anyway. |
| Extract the presentational core | **chosen** | `BlockList` owns no chrome (no header, no width, no scroll container, no data fetch), so the host decides all of that. |

The extraction also removed the last duplication between the two surfaces:
category order, colours, row layout and drag payload now exist once.

## The API

```ts
export interface BlockEntry {
  type: string;                         // registry key, used to create the block
  name: string;                         // display name
  category: Category;
  config: Record<string, number>;       // every param at its spec default
}

export interface BlockGroup {
  category: Category;
  blocks: BlockEntry[];
}

export function groupBlocks(
  registry: Record<string, BlockSpec>,
  query = "",                           // case-insensitive substring over name + type
): BlockGroup[];

export function BlockList({
  groups,
  onSelect,
}: {
  groups: BlockGroup[];
  onSelect: (entry: BlockEntry) => void;
});
```

Notes that matter to a host:

- `config` is ready-to-create defaults; it is also what the drag payload carries.
- Rows are HTML5 drag sources publishing `BLOCK_MIME` (see `ui/src/dnd.ts`).
  That behaviour is unchanged when the list is embedded in a modal.
- `groupBlocks` deliberately omits `I/O` and `Split/Merge` categories: they are
  graph wiring primitives, not pedals a player drops into a chain.
- Filtering lives in `groupBlocks`, never inside `BlockList`.

## How the modal hosts it

`AddBlockModal` supplies everything the list does not:

- **Chrome** — `.modal.picker` (520 px wide), a header with the target-channel
  chip and close button, and a footer with the match count and keyboard hints.
- **Data** — `groupBlocks(s.registry, blockQuery.value)`, so typing in
  `.picker-search` re-filters the same component the rack uses.
- **Selection semantics** — `pick(entry)` calls
  `addBlock(channelId, entry.type, entry.name)` and closes. Cancelling
  (Escape, backdrop, ×) never invokes `onSelect`.
- **Layout** — the modal is a flex column; `.picker-body` is `flex: 1` with
  `overflow-y: auto`. The list itself contributes no height or width
  assumptions, so no CSS changes were needed to embed it. There is no header to
  remove because `BlockList` has none.
- **Empty state** — owned by the host ("No blocks match …"), not the list.
- **Keyboard** — Enter adds the first match; Escape closes from anywhere.

Open/closed state and filter text live in `ui/src/state.ts`:
`addBlockFor: Signal<string | null>`, `blockQuery: Signal<string>`, and
`openBlockPicker(channel)` which resets the query before opening.

## Embedding checklist (for future browsers, e.g. the model browser picker)

1. Keep the list component presentational: data in via props, choice out via
   one `onSelect(entry)` callback.
2. Let the host own chrome, width, scrolling, filtering and empty states.
3. Put search/filter logic in a pure function next to the component, not in it.
4. Prove it renders standalone: feed it plain data with no app state, signals or
   bridge (the throwaway harness under `ui/standalone-tmp/` does this).
5. The host decides what "select" means (create + close, download, use-in-block)
   — the list must not know.

## Verification

- `cd ui && pnpm run build` — typechecks and builds green; reproduces the
  committed `ui/dist`.
- Standalone render (throwaway harness, `ui/standalone-tmp/`): `BlockList`
  renders a fake registry with no app shell, signals or engine; clicking a row
  fires `onSelect` with the full `BlockEntry`; a query filters the sections.
- Live app against the headless engine (`--headless`, output discarded): the
  rack lists every registry block grouped by category; opening the picker shows
  the identical rows; typing `delay` narrows them to *Delay* and *Pitch Delay*
  with the count updating; Escape closes with no block added.

## Known limits

- Dragging a row out of the modal onto a channel is not reachable: the modal
  backdrop covers the drop targets, so native drag events land on the modal.
  Click-to-add is the modal path; drag-and-drop remains a rack feature.
- `BlockList` shows no loading state of its own; the registry arrives with the
  state snapshot, and the app gates rendering on that snapshot being present.
