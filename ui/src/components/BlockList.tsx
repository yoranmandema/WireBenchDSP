// The block catalogue, shared by the left rack and the add-block picker. One
// renderer means both surfaces stay identical: category grouping, ordering,
// colour and row layout all come from here, so the picker really is the rack's
// browser reused as a modal.
//
// Rows are HTML5 drag sources. `dragstart` publishes a `BlockPayload` on the
// shared `BLOCK_MIME` type (see `dnd.ts`) and `dragend` clears the transient
// `.dragging` marker; the channel view is the drop target.

import { BlockIcon, Icon } from "./Icon";
import { catStyle } from "../theme";
import type { BlockSpec, Category } from "../protocol";
import { BLOCK_MIME, encodeBlockPayload, type BlockPayload } from "../dnd";

/**
 * Categories in display order. I/O and Split/Merge are deliberately absent:
 * they are graph wiring primitives (input/output nodes, the recombine summer),
 * not pedals a player drops into a chain.
 */
export const CATEGORY_ORDER: Category[] = [
  "Modelling",
  "Distortion",
  "Tone",
  "Dynamics",
  "Time",
  "Modulation",
  "Pitch",
  "Utility",
];

export interface BlockEntry {
  type: string;
  name: string;
  category: Category;
  /** Default parameter values, taken straight from the registry spec. */
  config: Record<string, number>;
}

export interface BlockGroup {
  category: Category;
  blocks: BlockEntry[];
}

/** Default configuration for a block: every parameter at its spec default. */
function defaultConfig(spec: BlockSpec): Record<string, number> {
  return Object.fromEntries(spec.params.map((p) => [p.id, p.default]));
}

/**
 * Group the registry by category in display order, dropping entries that do not
 * match `query` (case-insensitive substring over name and type). An empty
 * query lists everything. Insertion order within a category is preserved.
 */
export function groupBlocks(
  registry: Record<string, BlockSpec>,
  query = "",
): BlockGroup[] {
  const q = query.trim().toLowerCase();
  const byCategory = new Map<Category, BlockEntry[]>();
  for (const [type, spec] of Object.entries(registry)) {
    if (q && !`${spec.name} ${type}`.toLowerCase().includes(q)) continue;
    const list = byCategory.get(spec.category) ?? [];
    list.push({
      type,
      name: spec.name,
      category: spec.category,
      config: defaultConfig(spec),
    });
    byCategory.set(spec.category, list);
  }
  return CATEGORY_ORDER.filter((c) => byCategory.has(c)).map((c) => ({
    category: c,
    blocks: byCategory.get(c)!,
  }));
}

/** The drag in flight: its source row and the payload it published. */
interface BlockDrag {
  row: HTMLElement;
  payload: BlockPayload;
}

/** Outcome of the most recent browser drag. `dragend` writes it; the
 *  drop-target side and the edge-case tests read it. */
export const lastBlockDrag: { dropped: boolean; payload: BlockPayload | null } = {
  dropped: false,
  payload: null,
};

/** Module-level handle on the current drag. `dragend` has no other way back to
 * the row that started the gesture, so the source lives here for the duration. */
let dragSource: BlockDrag | null = null;

/** Publish `b` on the data transfer and mark its row as the drag source. */
function startDrag(e: DragEvent, b: BlockEntry) {
  const row = e.currentTarget as HTMLElement;
  const payload: BlockPayload = { type: b.type, name: b.name, config: b.config };
  const dt = e.dataTransfer;
  if (!dt) return;
  dt.setData(BLOCK_MIME, encodeBlockPayload(payload));
  // Plain-text mirror, so a target that only reads text still sees the name.
  dt.setData("text/plain", b.name);
  dt.effectAllowed = "copy";
  dragSource = { row, payload };
  lastBlockDrag.dropped = false;
  lastBlockDrag.payload = payload;
  row.classList.add("dragging");
}

/** Clear the source marker and record whether a target accepted the drop. */
function endDrag(e: DragEvent) {
  const src = dragSource;
  dragSource = null;
  src?.row.classList.remove("dragging");
  // `dropEffect` stays "none" unless a drop target accepted the drag and chose
  // an effect, so it is what separates a delivered drop from a cancelled one.
  lastBlockDrag.dropped = (e.dataTransfer?.dropEffect ?? "none") !== "none";
}

/**
 * The grouped catalogue itself: one section per category, one button per block.
 * Purely presentational — it reads `groups` and reports a click through
 * `onSelect(entry)`, so the same list serves the always-visible rack and the
 * modal picker. Grouping and filtering live in `groupBlocks`, not here.
 */
export function BlockList({
  groups,
  onSelect,
}: {
  groups: BlockGroup[];
  onSelect: (entry: BlockEntry) => void;
}) {
  return (
    <div class="block-list phone:flex phone:items-start phone:gap-4">
      {groups.map((g) => (
        <section key={g.category} class="flex flex-col gap-1 phone:flex-none">
          <h2 class="cat-scope flex items-center gap-1.5 text-[10px] tracking-[1.4px] font-semibold mb-1.5 text-cat phone:mb-1 phone:text-[9px] phone:tracking-[1px]" style={catStyle(g.category)}>
            <Icon category={g.category} size={12} />
            <span>{g.category}</span>
          </h2>
          {g.blocks.map((b) => (
            <button
              key={b.type}
              class="rack-item cat-scope w-full flex items-center gap-2 px-2.5 py-1.75 rounded-md text-left border border-transparent border-l-3 bg-linear-to-r from-cat-soft to-transparent cursor-grab transition-[background-color,border-color,border-left-color] duration-120 hover:bg-panel-2 hover:border-[#3b3e49] hover:border-l-cat phone:flex-none phone:w-[172px] phone:px-2 phone:py-1"
              style={catStyle(b.category)}
              draggable
              onDragStart={(e) => startDrag(e, b)}
              onDragEnd={endDrag}
              onClick={() => onSelect(b)}
              title={`Add ${b.name} (${b.type}) — or drag it into a channel`}
            >
              <span class="rack-icon grid place-items-center text-cat">
                <BlockIcon type={b.type} category={b.category} size={18} />
              </span>
              <span class="rack-text grid gap-[1px] min-w-0">
                <span class="rack-name text-[12px] font-medium">{b.name}</span>
              </span>
            </button>
          ))}
        </section>
      ))}
    </div>
  );
}