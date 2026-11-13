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
    <div class="knob-unit">
      <div
        class={`knob${dragging ? " dragging" : ""}`}
        style={{ "--angle": `${angle}deg`, "--sweep": `${t * 270}deg` }}
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onPointerCancel={up}
        onDblClick={() => defaultValue !== undefined && onChange(defaultValue)}
        title={`${name}: ${display} — drag, double-click to reset`}
      />
      <div class="knob-name">{name}</div>
      <div class="knob-value">{display}</div>
    </div>
  );
}
