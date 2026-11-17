// Cross-channel routing lines.
//
// A routing is a tap from one block's output to a block in another channel.
// It is drawn as a dashed amber curve from the source card's output dot to the
// destination card's input dot, above the channel rows. That is what makes it
// read as something added on top of the chain rather than as part of it: the
// in-channel flow is only the left-to-right card order and is never drawn.
//
// The overlay is a sibling of the channel rows inside `.chain-scroll` (which is
// the positioning context), so it is not clipped by the horizontal `.strip`
// scrollers and it scrolls with the panel. Every coordinate is measured live
// from the DOM — `getBoundingClientRect` of the two cards relative to the
// overlay — so a line follows its blocks when a strip scrolls, the panel
// scrolls, the window resizes, or the rig changes. Nothing about a routing's
// geometry is sent to the engine.

import { useEffect, useRef, useState } from "preact/hooks";
import { selectedRouting, selectRouting, state } from "../state";
import type { RoutingEndpoint, State } from "../protocol";

/** The app's amber accent. Routings are the one thing drawn in it, so a line
 *  never reads as part of a block's own category colour. */
const AMBER = "#e8a33d";

/** Arrowhead length and half-width, in px. */
const ARROW_LEN = 4;
const ARROW_HALF_WIDTH = 2.5;

/** S-curve shared by the live preview line and the real routings. */
export function routeCurve(x1: number, y1: number, x2: number, y2: number): string {
  const dx = x2 - x1;
  const bend = Math.abs(dx) < 8 ? 6 : Math.max(20, Math.min(Math.abs(dx) * 0.5, 90));
  return `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`;
}

/** The app's amber accent. */
export const ROUTING_AMBER = "#e8a33d";

/** The measured geometry of one line; styling is applied at render time. */
interface Line {
  id: string;
  d: string;
  /** Arrowhead polygon at the destination; null for a disabled routing. */
  arrow: string | null;
  disabled: boolean;
}

/**
 * Measure every routing that can be drawn.
 *
 * Endpoints are resolved against `state.channels` first, so a routing whose
 * block has been deleted is skipped rather than drawn to a stale card. Block
 * ids are unique across the rig, so one scan of the chain's cards is enough.
 */
function measure(overlay: HTMLElement): Line[] {
  const s = state.value;
  if (!s) return [];
  const box = overlay.getBoundingClientRect();
  const cards = new Map<string, HTMLElement>();
  overlay.parentElement
    ?.querySelectorAll<HTMLElement>("[data-block-id]")
    .forEach((el) => cards.set(el.dataset.blockId ?? "", el));

  const lines: Line[] = [];
  for (const r of s.routings ?? []) {
    const { source, dest } = r;
    if (!hasBlock(s, source) || !hasBlock(s, dest)) continue;
    const from = cards.get(source.block);
    const to = cards.get(dest.block);
    if (!from || !to) continue;

    const a = from.getBoundingClientRect();
    const b = to.getBoundingClientRect();
    // The cards carry their input/output dots just outside the border box
    // (`-left-1` / `-right-1`, 5px wide), so the line meets the dot, not the
    // card's edge.
    const x1 = a.right - box.left + 1.5;
    const y1 = a.top + a.height / 2 - box.top;
    const x2 = b.left - box.left - 1.5;
    const y2 = b.top + b.height / 2 - box.top;

    const dx = x2 - x1;
    // A horizontal S-curve. The destination is always entered from the left,
    // along its input dot, so the arrowhead points straight into the card.
    const bend = Math.abs(dx) < 8 ? 6 : Math.max(20, Math.min(Math.abs(dx) * 0.5, 90));
    const d = `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`;
    const arrow = r.enabled
      ? `${x2},${y2} ${x2 - ARROW_LEN},${y2 - ARROW_HALF_WIDTH} ${x2 - ARROW_LEN},${y2 + ARROW_HALF_WIDTH}`
      : null;

    lines.push({ id: r.id, d, arrow, disabled: !r.enabled });
  }
  return lines;
}

/** Does `endpoint` name a block that is still in the rig? */
function hasBlock(s: State, e: RoutingEndpoint): boolean {
  return (s.channels.find((c) => c.id === e.channel)?.blocks ?? []).some((b) => b.id === e.block);
}

/**
 * The routing overlay. Mounted once, inside the chain's scroll content, and
 * re-measured on every scroll, resize and rig change.
 */
export function RoutingLines() {
  const [lines, setLines] = useState<Line[]>([]);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let raf = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let last = "";

    // The overlay's own box, the chain container, the scrolling panel and
    // every strip: any of them changing size moves the cards.
    const watch = () => {
      const el = ref.current;
      if (!el) return;
      ro.observe(el);
      const chain = el.parentElement;
      if (!chain) return;
      ro.observe(chain);
      if (chain.parentElement) ro.observe(chain.parentElement);
      chain.querySelectorAll(".strip").forEach((strip) => ro.observe(strip));
    };

    const read = () => {
      const el = ref.current;
      if (!el) return;
      watch();
      const next = measure(el);
      const key = next.map((l) => `${l.id}|${l.d}|${l.arrow}`).join("\n");
      if (key === last) return;
      last = key;
      setLines(next);
    };

    const schedule = () => {
      if (!raf) raf = requestAnimationFrame(() => { raf = 0; read(); });
      // A tab that is not being painted produces no frames, so `rAF` alone
      // could leave the lines stale; the timer is the fallback.
      if (!timer) timer = setTimeout(() => { timer = null; read(); }, 120);
    };
    const ro = new ResizeObserver(schedule);

    const onScroll = () => schedule();
    const onResize = () => schedule();
    const detachState = state.subscribe(schedule);
    window.addEventListener("resize", onResize);
    // `capture` so a scroll in any scroller — the panel or a strip — re-measures.
    window.addEventListener("scroll", onScroll, true);
    // A freshly mounted card may still be moving; re-measure as layout settles.
    const timers = [50, 150, 400].map((t) => setTimeout(schedule, t));
    // The first measurement is synchronous, so the lines are on screen with the
    // first paint rather than a frame later.
    read();
    schedule();

    return () => {
      if (raf) cancelAnimationFrame(raf);
      if (timer) clearTimeout(timer);
      timers.forEach(clearTimeout);
      detachState();
      ro.disconnect();
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, []);

  const sel = selectedRouting.value;

  return (
    <div ref={ref} class="pointer-events-none absolute inset-0 z-10 overflow-hidden">
      {lines.map((l) => {
        const selected = sel === l.id;
        const opacity = l.disabled ? 0.35 : selected ? 1 : 0.7;
        return (
          <svg key={l.id} class="absolute inset-0 h-full w-full overflow-visible" data-routing-id={l.id}>
            <g class="group">
              <path
                d={l.d}
                fill="none"
                stroke={AMBER}
                stroke-width={selected ? 2.5 : 1.5}
                stroke-dasharray={selected ? undefined : "5 4"}
                opacity={opacity}
                class={selected ? undefined : "group-hover:opacity-90"}
                style={{
                  filter: selected
                    ? "drop-shadow(0 0 6px rgba(232,163,61,0.95))"
                    : "drop-shadow(0 0 4px rgba(232,163,61,0.55))",
                }}
              />
              {l.arrow && <polygon points={l.arrow} fill={AMBER} opacity={opacity} />}
              {/* Invisible wide stroke: the click target. The overlay itself is
                  `pointer-events: none`, so only this stroke takes clicks. */}
              <path
                d={l.d}
                fill="none"
                stroke={AMBER}
                stroke-width={12}
                opacity={0}
                class="cursor-pointer [pointer-events:stroke]"
                onClick={() => selectRouting(l.id)}
              >
                <title>{`Routing ${l.id}`}</title>
              </path>
            </g>
          </svg>
        );
      })}
    </div>
  );
}
