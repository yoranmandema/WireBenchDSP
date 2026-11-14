// One stroke-drawn glyph per category. Inline SVG, `currentColor`, so an icon
// picks up the category colour from its parent.

import type { JSX } from "preact";
import type { Category } from "../protocol";

const GLYPHS: Record<Category, JSX.Element> = {
  // Amp head over a speaker.
  Modelling: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <circle cx="12" cy="14" r="3.4" />
      <path d="M7 7.5h3M7 10h3" />
    </>
  ),
  // A sine wave clipped flat at the top.
  Distortion: <path d="M3 12h3l2.5-7 3 14 2.5-11 2.5 7H21" />,
  // Three EQ faders.
  Tone: (
    <>
      <path d="M6 4v16M12 4v16M18 4v16" />
      <circle cx="6" cy="9" r="2" />
      <circle cx="12" cy="14" r="2" />
      <circle cx="18" cy="7" r="2" />
    </>
  ),
  // Compressor knee.
  Dynamics: (
    <>
      <path d="M3 19V5" />
      <path d="M3 19h9l4-9h5" />
    </>
  ),
  // Clock face.
  Time: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7.5V12l3 2" />
    </>
  ),
  // Sine wave with a wobble.
  Modulation: <path d="M2.5 12c2-6.5 4-6.5 6 0s4 6.5 6 0 4-6.5 6 0" />,
  // Rising arrow.
  Pitch: (
    <>
      <path d="M4 18l5-6.5 4 3.5L20 7" />
      <path d="M16.5 7H20v3.5" />
    </>
  ),
  // Gear.
  Utility: (
    <>
      <circle cx="12" cy="12" r="3.2" />
      <path d="M12 3.5v2.6M12 17.9v2.6M3.5 12h2.6M17.9 12h2.6M6 6l1.8 1.8M16.2 16.2L18 18M18 6l-1.8 1.8M7.8 16.2L6 18" />
    </>
  ),
  // In / out arrows.
  "I/O": <path d="M4 8h9l-3-3M20 16H11l3 3" />,
  // One line splitting into two.
  "Split/Merge": (
    <>
      <path d="M12 3v7" />
      <path d="M12 10l-6.5 6.5M12 10l6.5 6.5" />
      <path d="M5.5 16.5V20M18.5 16.5V20" />
    </>
  ),
  // Slider with a handle.
  Control: (
    <>
      <path d="M5 19V5M19 19V5" />
      <path d="M5 12h14" />
      <circle cx="12" cy="12" r="2" />
    </>
  ),
};

/** A category glyph. `size` is the square edge in px. */
export function Icon({
  category,
  size = 16,
}: {
  category: Category;
  size?: number;
}) {
  return (
    <svg
      class="icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width={1.7}
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {GLYPHS[category] ?? GLYPHS.Utility}
    </svg>
  );
}
