// The drag-and-drop contract between the block browser (the drag source) and
// the channel view (the drop target).
//
// Three transports, one payload:
//
//  1. **HTML5 drag** (`dragstart` → `drop`) — the mouse path. The rack row
//     publishes a `BlockPayload` on the shared `BLOCK_MIME` type. During
//     `dragover` the payload itself is not readable (browsers expose only the
//     type list), so a target decides whether to accept a drag from
//     `dataTransfer.types` alone.
//  2. **Pointer** (`pointerdown`/`move`/`up`) — the touch path *and* the
//     reorder path inside a channel. Both are the same three methods; only the
//     start condition differs (a long press vs. a drag threshold). The timing
//     lives in `touch.ts`; the gesture itself lives in `Chain.tsx`, which owns
//     the insertion slots.
//  3. **Keyboard** — no gesture at all: Enter/Space on a rack row selects the
//     type (`state.ts` `selectedBlockType`), then the channel's
//     "Add selected block" button inserts it at the end.
//
// Nothing here assumes a mouse. `touch-action: none` on the rows and cards is
// what lets a touch drag reach `pointermove` instead of scrolling the panel.

export const BLOCK_MIME = "application/x-block-type";

/** How long a touch press must hold before it turns into a drag. */
export const LONG_PRESS_MS = 350;
/** How far a touch may travel during a long press and still count as one. */
export const LONG_PRESS_SLOP_PX = 10;
/** How far a mouse must move before a plain press becomes a drag. */
export const DRAG_THRESHOLD_PX = 4;

/** One block type, offered by the rack and the picker. */
export interface BlockEntry {
  type: string;
  name: string;
  category: import("./protocol").Category;
  /** Default parameter values, taken straight from the registry spec. */
  config: Record<string, number>;
}

export interface BlockPayload {
  /** Registry id, e.g. `amp.basic`. */
  type: string;
  /** Display name, e.g. `Amp`. */
  name: string;
  /** Default configuration. The engine ignores it today; part of the contract. */
  config?: Record<string, unknown>;
}

export function encodeBlockPayload(p: BlockPayload): string {
  return JSON.stringify({ type: p.type, name: p.name, config: p.config ?? {} });
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return !!v && typeof v === "object" && !Array.isArray(v);
}

/** Why a drop was refused. One code per case the tests assert on. */
export type DropRejection = "no-payload" | "bad-type" | "unknown-type";

/**
 * Read a block payload from a drop.
 *
 * Only `BLOCK_MIME` (or a JSON object smuggled through `text/plain`) is
 * accepted; arbitrary text is not a block and yields null. Field names are
 * tolerant (`type`/`typeId`, `name`/`label`) so a source written against an
 * earlier sketch of the contract still lands.
 */
export function decodeBlockPayload(dt: DataTransfer | null): BlockPayload | null {
  if (!dt) return null;
  const raw = dt.getData(BLOCK_MIME) || dt.getData("text/plain");
  if (!raw) return null;
  let p: Record<string, unknown>;
  try {
    p = JSON.parse(raw) as Record<string, unknown>;
  } catch {
    return null;
  }
  if (!isRecord(p)) return null;
  const type = p.type ?? p.typeId ?? p.blockType;
  if (typeof type !== "string" || !type) return null;
  const name = p.name ?? p.label;
  return {
    type,
    name: typeof name === "string" && name ? name : type,
    config: isRecord(p.config) ? p.config : {},
  };
}

/**
 * Validate a dragged payload against the registry. `null` means it is
 * droppable; otherwise the reason. The caller warns and does nothing.
 *
 * Split out from the drop handler so a refusal is testable without a
 * `DataTransfer`: the two interesting cases are "no usable payload" and "a
 * payload whose type id the engine does not offer".
 */
export function rejectDrop(
  payload: BlockPayload | null,
  registry: Record<string, unknown>,
): DropRejection | null {
  if (!payload) return "no-payload";
  if (!payload.type) return "bad-type";
  if (!registry[payload.type]) return "unknown-type";
  return null;
}

/** Whether this drag carries a block payload — the only thing we accept. */
export function isBlockDrag(dt: DataTransfer | null): boolean {
  return !!dt && Array.from(dt.types).includes(BLOCK_MIME);
}

// --- HTML5 drag source (rack rows) -----------------------------------------

/** The row currently being dragged, if any.
 *
 *  `dragend` has no other way back to the row that started the gesture, so the
 *  source lives here for the duration and is cleared on every path out. */
let dragSource: { row: HTMLElement; payload: BlockPayload } | null = null;

/** Outcome of the most recent browser drag. `dragend` writes it; the
 *  drop-target side and the edge-case tests read it. */
export const lastBlockDrag: { dropped: boolean; payload: BlockPayload | null } = {
  dropped: false,
  payload: null,
};

/** Publish `payload` on the data transfer and mark its row as the drag source. */
export function startDrag(e: DragEvent, row: HTMLElement, payload: BlockPayload): void {
  const dt = e.dataTransfer;
  if (!dt) return;
  dt.setData(BLOCK_MIME, encodeBlockPayload(payload));
  // Plain-text mirror, so a target that only reads text still sees the name.
  dt.setData("text/plain", payload.name);
  dt.effectAllowed = "copy";
  dragSource = { row, payload };
  lastBlockDrag.dropped = false;
  lastBlockDrag.payload = payload;
  row.classList.add("dragging");
}

/**
 * Clear the source marker and record whether a target accepted the drop.
 *
 * Always called from `dragend`, whatever the outcome: a cancelled drag, a drop
 * on empty space and a delivered drop all end here, so the `.dragging` marker
 * and the stored payload cannot outlive the gesture.
 */
export function endDrag(e: DragEvent): void {
  const src = dragSource;
  dragSource = null;
  src?.row.classList.remove("dragging");
  // `dropEffect` stays "none" unless a drop target accepted the drag and chose
  // an effect, so it is what separates a delivered drop from a cancelled one.
  lastBlockDrag.dropped = (e.dataTransfer?.dropEffect ?? "none") !== "none";
}

/** Forget the stored drag source, without touching any outcome flags.
 *  Used when a gesture is abandoned before `dragend` can run. */
export function resetDragState(): void {
  dragSource?.row.classList.remove("dragging");
  dragSource = null;
}
