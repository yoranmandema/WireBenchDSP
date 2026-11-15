// Selection highlight rendering.
//
// A selected block card used to draw its ring and glow with `box-shadow`,
// which is painted inside the card's own box — and therefore clipped by any
// ancestor with `overflow` other than `visible`. In this UI the chain scrolls
// vertically (`.chain-panel`, `overflow: auto`) and each channel strip scrolls
// horizontally (`.strip`, `overflow-x: auto`), so a card near either edge had
// its ring and glow cut off.
//
// So the ring is not painted on the card at all. It is drawn by a single
// overlay element that lives in `.app` (which has no `overflow` clipping), is
// positioned with `position: fixed` at the card's viewport rectangle, and
// follows the card on scroll and resize. The card itself keeps only a 1px
// inset ring, which sits inside its own border box and so cannot be clipped
// by anything.
//
// The overlay is `pointer-events: none` and `z-index: 30`, below the modals
// (40) and the toast (50), so it never intercepts a click or covers a dialog.

import { useEffect, useState } from "preact/hooks";
import { selected, state } from "./state";

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
  /** The selected card's category hue, so the ring matches the card. */
  hue: number;
  /** Card border width; the ring straddles the border box by this much. */
  border: number;
  /** Card border radius, so the ring's corners follow the card's. */
  radius: number;
}

/** The currently selected card element, or null. */
function cardEl(): HTMLElement | null {
  return (document.querySelector(".block-card.selected") as HTMLElement | null) ?? null;
}

/** `--cat-h` from the card's inline style, or the stylesheet default (222). */
function cardHue(el: HTMLElement): number {
  const raw = el.style.getPropertyValue("--cat-h");
  const n = raw === "" ? NaN : Number(raw);
  return Number.isFinite(n) ? n : 222;
}

/** The card's border width and radius, which the ring straddles. */
function cardMetrics(el: HTMLElement): { border: number; radius: number } {
  const s = getComputedStyle(el);
  return { border: parseFloat(s.borderTopWidth) || 0, radius: parseFloat(s.borderTopLeftRadius) || 0 };
}

/**
 * Track the selected card's viewport rectangle and report it.
 *
 * `onRect` fires again whenever the card moves: scrolling the chain or a
 * strip, resizing the window, or the layout settling after a state update.
 * Returns a detach function.
 */
export function trackSelection(onRect: (r: Rect | null) => void): () => void {
  let raf = 0;
  let last = "";

  const read = () => {
    raf = 0;
    const el = cardEl();
    if (!el) {
      if (last !== "") {
        last = "";
        onRect(null);
      }
      return;
    }
    const r = el.getBoundingClientRect();
    const m = cardMetrics(el);
    const key = `${Math.round(r.left)}|${Math.round(r.top)}|${Math.round(r.width)}|${Math.round(r.height)}|${cardHue(el)}`;
    if (key === last) return;
    last = key;
    onRect({
      left: r.left,
      top: r.top,
      width: r.width,
      height: r.height,
      hue: cardHue(el),
      border: m.border,
      radius: m.radius,
    });
  };

  const schedule = () => {
    if (!raf) raf = requestAnimationFrame(read);
  };

  // Selection and rig changes move the card; scroll and resize move it too.
  // `capture: true` so a scroll anywhere in the app (chain, strip, rack,
  // editor) repositions the ring.
  const onScroll = () => schedule();
  const onResize = () => schedule();

  const detachState = state.subscribe(schedule);
  const detachSel = selected.subscribe(schedule);
  window.addEventListener("resize", onResize);
  window.addEventListener("scroll", onScroll, true);
  // One read after layout settles, so a freshly rendered card gets its ring.
  const timers = [0, 50, 150, 400].map((t) => setTimeout(schedule, t));
  schedule();

  return () => {
    cancelAnimationFrame(raf);
    timers.forEach(clearTimeout);
    detachState();
    detachSel();
    window.removeEventListener("resize", onResize);
    window.removeEventListener("scroll", onScroll, true);
  };
}

/**
 * The selection ring overlay. Mounted once, high in the tree, so it is never
 * inside a clipping container. Re-renders only when the measured rect
 * changes, which keeps it cheap during a scroll.
 */
export function SelectionRing() {
  const [rect, setRect] = useState<Rect | null>(null);

  useEffect(() => trackSelection(setRect), []);

  if (!rect) return null;
  return (
    <div
      class="selection-ring"
      aria-hidden="true"
      style={{
        // The ring sits *outside* the card's border box, like the outer ring
        // it replaces. It is `border`-wide outside so it abuts the card's
        // border instead of hiding under the card's own background, and the
        // glow extends further still.
        left: `${rect.left - rect.border}px`,
        top: `${rect.top - rect.border}px`,
        width: `${rect.width + rect.border * 2}px`,
        height: `${rect.height + rect.border * 2}px`,
        borderRadius: `${rect.radius + rect.border}px`,
        ["--cat-h" as string]: String(rect.hue),
        ["--cat" as string]: `hsl(${rect.hue} 62% 58%)`,
      }}
    />
  );
}
