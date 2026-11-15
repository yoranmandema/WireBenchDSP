// The HTML5 drag-and-drop contract between the block browser (the drag source)
// and the channel view (the drop target).
//
// One MIME type and one JSON shape, so the two sides can be built and tested
// independently. The source writes `BLOCK_MIME` on `dragstart`; the target
// reads it on `drop`. During `dragover` the payload itself is not readable
// (browsers expose only the type list), so the target decides whether to accept
// a drag from `dataTransfer.types` alone.

export const BLOCK_MIME = "application/x-block-type";

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

/** Whether this drag carries a block payload — the only thing we accept. */
export function isBlockDrag(dt: DataTransfer | null): boolean {
  return !!dt && Array.from(dt.types).includes(BLOCK_MIME);
}
