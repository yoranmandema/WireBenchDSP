import { useRef, useState } from "preact/hooks";

interface KnobProps {
  name: string;
  value: number;
  min: number;
  max: number;
  defaultValue?: number;
  unit?: string;
  format?: (v: number) => string;
  onChange: (v: number) => void;
}

/** Geometry of the indicator ring around the knob face. */
const RING_R = 30; // ring centreline radius, px (the face itself is r=27)
const RING_W = 4; // ring thickness
const START = 135; // SVG degrees clockwise from 3 o'clock == 225deg on a clock face

/** Compact readout for the arc's end stops (min / max).
 *
 *  Bare numbers, no unit: the pills sit between the ring and the parameter
 *  name at 6.5px, and a suffix like "cents" would make them wide enough to
 *  reach the next knob. The knob's own value display carries the unit. */
function endLabel(v: number): string {
  const a = Math.abs(v);
  return a >= 100 ? v.toFixed(0) : a >= 10 ? v.toFixed(1) : v.toFixed(2);
}

/** A rotary knob. Drag vertically, double-click to reset. */
export function Knob({
  name,
  value,
  min,
  max,
  defaultValue,
  unit = "",
  format,
  onChange,
}: KnobProps) {
  const [dragging, setDragging] = useState(false);
  const start = useRef({ y: 0, v: 0 });

  const span = max - min || 1;
  const t = Math.min(1, Math.max(0, (value - min) / span));
  // 270 degrees of travel, from -135deg to +135deg.
  const angle = -135 + t * 270;
  const display = format ? format(value) : `${value.toFixed(2)}${unit}`;

  const down = (e: PointerEvent) => {
    e.preventDefault();
    (e.target as Element).setPointerCapture?.(e.pointerId);
    start.current = { y: e.clientY, v: value };
    setDragging(true);
  };
  const move = (e: PointerEvent) => {
    if (!dragging) return;
    const dy = start.current.y - e.clientY;
    const next = start.current.v + (dy / 150) * span;
    onChange(Math.min(max, Math.max(min, next)));
  };
  const up = () => setDragging(false);

  return (
    <div class="knob-unit flex flex-col items-center w-full gap-1 p-0">
      <div
        class={`knob relative size-[var(--ui-knob)] rounded-full cursor-ns-resize touch-none ${dragging ? "dragging" : ""}`}
        style={{ "--angle": `${angle}deg` }}
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onPointerCancel={up}
        onDblClick={() => defaultValue !== undefined && onChange(defaultValue)}
        title={`${name}: ${display} — drag, double-click to reset`}
      >
        {/* The ring is SVG, not a masked conic gradient: exact geometry, and it
            renders identically in every webview. pathLength=100 makes the dash
            numbers percentages of the circumference; 75 = the 270deg window. */}
        <svg class="knob-arc absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 pointer-events-none" viewBox="0 0 64 64" width="64" height="64" aria-hidden="true">
          <circle cx="32" cy="32" r={RING_R} fill="none" stroke="#2f323b" strokeWidth={RING_W} pathLength={100} strokeDasharray="75 100" transform={`rotate(${START} 32 32)`} />
          <circle cx="32" cy="32" r={RING_R} fill="none" stroke="var(--amber)" strokeWidth={RING_W} strokeLinecap="round" pathLength={100} strokeDasharray={`${t * 75} 100`} transform={`rotate(${START} 32 32)`} />
        </svg>
        {/* End stops, sitting at the two ends of the travel arc. */}
        <span class="knob-end lo absolute text-pico leading-none text-muted tabular-nums bg-well border border-line-soft rounded-xs pointer-events-none" style={{ right: `calc(50% + var(--space-6))`, top: `calc(50% + var(--space-6))`, padding: 0 }}>{endLabel(min)}</span>
        <span class="knob-end hi absolute text-pico leading-none text-muted tabular-nums bg-well border border-line-soft rounded-xs pointer-events-none" style={{ left: `calc(50% + var(--space-6))`, top: `calc(50% + var(--space-6))`, padding: 0 }}>{endLabel(max)}</span>
      </div>
      <div class="knob-name text-center text-2xs tracking-[0.8px] text-muted font-semibold uppercase">{name}</div>
      <div class="knob-value text-xs text-text bg-inset border border-line-soft rounded-sm min-w-[56px] text-center tabular-nums px-2">{display}</div>
    </div>
  );
}