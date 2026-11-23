import { useMemo, useState } from "preact/hooks";
import { Graph, type ChartPads, type GraphAxes, type GraphOverlay } from "./Graph";
import type { JSX } from "preact";

/** One multiband-compressor band, as stored in the UI's display state. */
export interface CompBand {
  id: string;
  /** Frequency of the vertical crossover marker (upper boundary of this band). */
  crossover: number;
  threshold: number; // -60..0 dB
  gainReduction: number; // 0..-30 dB
  gain: number; // -12..+12 dB
}

/** 3-band default: ISO 1/1-octave crossovers at 200 Hz and 4 kHz. */
export const DEFAULT_BANDS: CompBand[] = [
  { id: "band-1", crossover: 200, threshold: -20, gainReduction: 0, gain: 0 },
  { id: "band-2", crossover: 4000, threshold: -20, gainReduction: 0, gain: 0 },
  { id: "band-3", crossover: 20000, threshold: -20, gainReduction: 0, gain: 0 },
];

/** The dynamics category hue (green, per ui/src/theme.ts). */
const MULTIBAND_COLOR = "hsl(152 62% 58%)";

/**
 * Evaluate the piecewise-linear gain-reduction curve across the band regions
 * at 200 log-spaced frequencies.
 *
 * The GR curve is a stepped function: each band's `gainReduction` applies
 * across that band's frequency region. 0 dB (no reduction) is the flat baseline.
 */
function multibandGr(bands: readonly CompBand[]): number[] {
  const data = new Array<number>(200);
  for (let i = 0; i < 200; i++) {
    const t = i / 199;
    const f = 20 * Math.pow(20000 / 20, t);
    let gr = 0;
    for (let b = 0; b < bands.length; b++) {
      const regionStart = b === 0 ? 20 : bands[b - 1].crossover;
      const regionEnd = b === bands.length - 1 ? 20000 : bands[b].crossover;
      if (f >= regionStart && f <= regionEnd) {
        gr = bands[b].gainReduction;
        break;
      }
    }
    data[i] = gr;
  }
  return data;
}

/** Wrap a raw overlay render function as the { id, render } overlay object. */
const ov = (
  id: string,
  render: (w: number, h: number, axes: GraphAxes<number>, pads: ChartPads) => JSX.Element,
): GraphOverlay<number> => ({ id, render });

/**
 * Vertical dashed crossover markers, one per band, with in-chart labels.
 *
 * The marker for band `i` sits at the band's upper boundary; the label is the
 * crossover frequency. The band's handle/region already reflects the curve,
 * so this overlay adds the boundary ticks.
 */
function multibandCrossoverMarkers(
  w: number,
  h: number,
  _axes: GraphAxes<number>,
  pads: ChartPads,
): JSX.Element {
  const { left, top, right, bottom } = pads;
  const chartW = w - left - right;
  const chartH = h - top - bottom;
  return (
    <g>
      {DEFAULT_BANDS.map((band) => {
        const x = left + (Math.log10(band.crossover / 20) / Math.log10(20000 / 20)) * chartW;
        return (
          <g key={band.id}>
            <line
              x1={x}
              y1={top}
              x2={x}
              y2={top + chartH}
              stroke="var(--color-amber)"
              strokeWidth={1}
              strokeDasharray="4 4"
              opacity={0.85}
            />
            <text
              x={x}
              y={top - 8}
              textAnchor="middle"
              fill="var(--color-muted)"
              fontSize={9}
            >
              {fmt(band.crossover)} Hz
            </text>
          </g>
        );
      })}
      <text
        x={left + chartW / 2}
        y={top + chartH + 34}
        textAnchor="middle"
        fill="var(--color-muted)"
        fontSize={9}
      >
        crossovers
      </text>
    </g>
  );
}

function fmt(f: number): string {
  return f >= 1000 ? `${(f / 1000).toFixed(1)}k` : String(f);
}

interface MultibandCompressorEditorProps {
  /** Display bands owned by this editor (the graph is display-only). */
  bands: readonly CompBand[];
  /** Called with a shallow copy of the band list whenever a band changes. */
  onBandChange: (bands: readonly CompBand[]) => void;
}

const clamp = (v: number, min: number, max: number): number => Math.min(max, Math.max(min, v));

/** Format a dB value for the band-table readout (normalises -0). */
function formatDb(v: number): string {
  const n = Math.round(v * 10) / 10;
  return `${Number.isNaN(n) ? "0" : n.toFixed(1)} dB`;
}

export function MultibandCompressorEditor({ bands, onBandChange }: MultibandCompressorEditorProps) {
  // Local state owns the display bands; the prop seeds it.
  const [localBands, setLocalBands] = useState<CompBand[]>(() =>
    bands.map((b) => ({ ...b })),
  );

  const grData = useMemo(() => multibandGr(localBands), [localBands]);

  // Update one band's field with clamping, then notify the parent.
  const updateBand = (index: number, field: keyof CompBand, value: number): void => {
    const next = localBands.map((b, i) => {
      if (i !== index) return b;
      const clamped =
        field === "threshold"
          ? clamp(value, -60, 0)
          : field === "gainReduction"
            ? clamp(value, -30, 0)
            : field === "gain"
              ? clamp(value, -12, 12)
              : clamp(value, 20, 20000);
      return { ...b, [field]: clamped };
    }) as CompBand[];
    setLocalBands(next);
    onBandChange(next);
  };

  // Per-band control row: crossover (fixed display), threshold, GR, band gain.
  const bandRows = localBands.map((band, i) => (
    <div class="multiband-band-row flex flex-wrap items-center gap-x-3 gap-y-1" key={band.id}>
      <span class="text-[10px] font-semibold uppercase tracking-[0.6px] text-muted">
        Band {i + 1}
      </span>
      <span class="text-[10px] font-mono tabular-nums text-muted">
        crossover {fmt(band.crossover)} Hz
      </span>
      <div class="multiband-slider flex flex-1 min-w-0 gap-x-1">
        <label class="flex-1 min-w-0 text-right text-[9px] font-mono tabular-nums text-muted">
          {band.id}
        </label>
        <input
          class="multiband-slider-input flex-1 min-w-0 cursor-pointer rounded border border-line bg-inset px-1 py-0.5 text-[9px] font-mono tabular-nums text-text"
          type="range"
          min={-60}
          max={0}
          step={0.5}
          value={band.threshold}
          aria-label={`Band ${i + 1} threshold`}
          onInput={(e) => updateBand(i, "threshold", parseFloat((e.target as HTMLInputElement).value))}
        />
        <span class="w-[36px] text-right text-[9px] font-mono tabular-nums text-muted">
          {formatDb(band.threshold)}
        </span>
      </div>
      <div class="multiband-slider flex flex-1 min-w-0 gap-x-1">
        <label class="flex-1 min-w-0 text-right text-[9px] font-mono tabular-nums text-muted">
          GR
        </label>
        <input
          class="multiband-slider-input flex-1 min-w-0 cursor-pointer rounded border border-line bg-inset px-1 py-0.5 text-[9px] font-mono tabular-nums text-text"
          type="range"
          min={-30}
          max={0}
          step={0.5}
          value={band.gainReduction}
          aria-label={`Band ${i + 1} gain reduction`}
          onInput={(e) => updateBand(i, "gainReduction", parseFloat((e.target as HTMLInputElement).value))}
        />
        <span class="w-[36px] text-right text-[9px] font-mono tabular-nums text-muted">
          {formatDb(band.gainReduction)}
        </span>
      </div>
      <div class="multiband-slider flex flex-1 min-w-0 gap-x-1">
        <label class="flex-1 min-w-0 text-right text-[9px] font-mono tabular-nums text-muted">
          gain
        </label>
        <input
          class="multiband-slider-input flex-1 min-w-0 cursor-pointer rounded border border-line bg-inset px-1 py-0.5 text-[9px] font-mono tabular-nums text-text"
          type="range"
          min={-12}
          max={12}
          step={0.1}
          value={band.gain}
          aria-label={`Band ${i + 1} band gain`}
          onInput={(e) => updateBand(i, "gain", parseFloat((e.target as HTMLInputElement).value))}
        />
        <span class="w-[36px] text-right text-[9px] font-mono tabular-nums text-muted">
          {formatDb(band.gain)}
        </span>
      </div>
    </div>
  ));

  return (
    <div class="multiband-compressor-editor">
      <div class="multiband-compressor-graph">
        <Graph
          axes={{
            x: { type: "log", min: 20, max: 20000, units: "Hz", ticks: 8 },
            y: { type: "linear", min: -30, max: 0, units: "dB", ticks: 6 },
          }}
          dataDomain={[20, 20000]}
          series={[{
            key: "multiband-gr",
            label: "Gain reduction",
            color: MULTIBAND_COLOR,
            kind: "area",
            data: grData,
          }]}
          overlays={[ov("multiband-crossover", multibandCrossoverMarkers)]}
          xLabel="Frequency"
          yLabel="Gain reduction"
          zeroLine
        />
      </div>

      {/* Belt-and-braces band table with per-band sliders. */}
      <div class="multiband-band-table border-t border-line-soft pt-3">
        <div class="flex items-center justify-between gap-2 text-[9px] font-semibold uppercase tracking-[0.6px] text-muted">
          <span class="flex-1">Band</span>
          <span class="flex-1 text-right">Threshold</span>
          <span class="flex-1 text-right">GR</span>
          <span class="flex-1 text-right">Band gain</span>
        </div>
        {bandRows}
      </div>
    </div>
  );
}
