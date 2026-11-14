// Visual identity per block category. One hue per category, used for the card
// accent, the rack chips, the icons and the editor header, so a signal chain
// reads as colour-coded at a glance.

import type { JSX } from "preact";
import type { Category } from "./protocol";

/** Hue (0-360) for each category. Chosen to stay legible on the dark theme. */
export const HUES: Record<Category, number> = {
  Modelling: 268, // violet — amps and cabs
  Distortion: 10, // ember red
  Tone: 42, // gold
  Dynamics: 152, // green
  Time: 206, // blue — delay/reverb
  Modulation: 306, // magenta
  Pitch: 186, // cyan
  Utility: 222, // slate blue
  "I/O": 0, // neutral
  "Split/Merge": 96, // olive
  Control: 24, // orange
};

export function hue(category: Category): number {
  return HUES[category] ?? 220;
}

/** Set `--cat-h` on an element so its descendants can derive the palette. */
export function catStyle(category: Category): JSX.CSSProperties {
  return { "--cat-h": String(hue(category)) } as unknown as JSX.CSSProperties;
}
