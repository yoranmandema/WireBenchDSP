import { useMemo } from "preact/hooks";
import type { Block, BlockSpec } from "../protocol";
import { ctrlName, emptyNote } from "../ui";
import { Knob } from "./Knob";
import { BandStrip } from "./BandStrip";
import { Format, ALL_PARAMS, activeBandIds, type ParamId } from "../params";
import {
  LocalParamSource,
  useMultibandParams,
  useMultibandMetersSource,
} from "../hooks/useParams";
import { MultibandMeter } from "./MultibandMeter";

/** Format a value by its schema unit (shared by the global knobs). */
function formatByUnit(unit: string, v: number): string {
  switch (unit) {
    case "dB":
      return Format.db(v);
    case "ms":
      return Format.ms(v);
    case "x":
      return Format.ratio(v);
    case "Hz":
      return Format.hz(v);
    case "%":
      return Format.percent(v);
    default:
      return Format.linear(v);
  }
}

/**
 * The multiband compressor's control panel.
 *
 * A single block in the channel strip. Renders the global controls (bands,
 * crossover frequencies, output gain, dry/wet), one per-band strip (threshold /
 * ratio / attack / release / makeup plus bypass / solo) per active band, and a
 * per-band gain-reduction meter region.
 *
 * Binding: every control reads from a `ParamSource` and commits through
 * `setValue` — the same seam every other editor uses via `setParam`. With no
 * engine present the panel is driven by `LocalParamSource` / `LocalMetersSource`
 * (in-memory stubs), so it renders and can be tested headless. Connecting the
 * engine is a one-line swap: pass an `EngineParamSource` that reads the rig
 * snapshot and writes the `setParam` command.
 */
export function MultibandCompressorEditor({
  channel,
  block,
  spec,
}: {
  channel: string;
  block: Block;
  spec: BlockSpec;
}) {
  const bands = block.params.bands ?? 3;
  const activeBands = activeBandIds(bands);

  // Resolve every param from the block's params map (schema defaults fill the
  // gaps until the engine lands). Values are the single source of truth for
  // the UI; `setValue` commits through the source.
  const params = useMemo(() => {
    const p = {} as Record<ParamId, number>;
    for (const schema of ALL_PARAMS) {
      p[schema.id] = block.params[schema.id] ?? schema.default;
    }
    return p;
  }, [block]);
  const meters = useMultibandMetersSource();
  const source = useMemo(() => new LocalParamSource(params), [params]);
  const { values, setValue } = useMultibandParams(source, block);

  const g = (id: ParamId): number => values[id];

  return (
    <div class="multiband-editor flex flex-col w-full min-w-0 gap-3" data-channel={channel}>
      {/* Identity banner: block name, type, bypass. */}
      <div
        class="cat-scope grid grid-cols-[minmax(0,1fr)] items-center justify-between gap-1 pl-3 pr-2 pt-2 min-h-[var(--ui-touch)]"
        style={spec ? { "--cat-h": String(152) } : undefined}
      >
        <div class="flex min-w-0 items-center gap-2">
          <span class="text-base font-semibold truncate" title={block.name}>
            {block.name}
          </span>
          <span class="editor-type text-[10px] tracking-[0.8px] text-muted">
            dynamics.multiband_compressor
          </span>
        </div>
        {block.bypass && (
          <span class="editor-bypass flex-none px-2 text-[9px] text-amber border border-amber-dim rounded-sm font-medium">
            bypassed
          </span>
        )}
      </div>

      {/* Global controls: bands, crossover frequencies, output gain, dry/wet. */}
      <div class="global-controls editor-controls grid grid-cols-[repeat(auto-fit,minmax(var(--ui-ctrl),1fr))] content-center justify-center gap-x-6 gap-y-4 min-w-0 w-full">
        <BandsControl value={g("bands")} onChange={(v) => setValue("bands", v)} />
        <div class="grid grid-cols-[repeat(auto-fit,minmax(var(--ui-ctrl),1fr))] content-center justify-center gap-x-6 gap-y-4 min-w-0 w-full">
          <ParamKnob
            id="output_gain"
            label="Output gain"
            value={g("output_gain")}
            onChange={(v) => setValue("output_gain", v)}
          />
          <ParamKnob
            id="dry_wet"
            label="Dry/Wet"
            value={g("dry_wet")}
            onChange={(v) => setValue("dry_wet", v)}
          />
        </div>
      </div>

      {/* Crossover axis: a log-scale axis with the band markers. */}
      <div class="crossover-axis">
        <div class="crossover-track relative" style={{ height: 28, padding: "0 2px", overflow: "hidden" }}>
          {(["crossover_1", "crossover_2", "crossover_3"] as const).map((id) => (
            <Marker key={id} value={g(id)} label={`${id}: ${Format.hz(g(id))}`} />
          ))}
        </div>
        <div class="crossover-controls editor-controls grid grid-cols-[repeat(auto-fit,minmax(var(--ui-ctrl),1fr))] content-center justify-center gap-x-6 gap-y-4 min-w-0 w-full">
          <ParamKnob
            id="crossover_1"
            label="Crossover 1"
            value={g("crossover_1")}
            onChange={(v) => setValue("crossover_1", v)}
          />
          <ParamKnob
            id="crossover_2"
            label="Crossover 2"
            value={g("crossover_2")}
            onChange={(v) => setValue("crossover_2", v)}
          />
          <ParamKnob
            id="crossover_3"
            label="Crossover 3"
            value={g("crossover_3")}
            onChange={(v) => setValue("crossover_3", v)}
          />
        </div>
      </div>

      {/* Per-band parameter strips. */}
      <div class="bands">
        {activeBands.map((band) => (
          <BandStrip key={band} band={band} values={values} onParamChange={setValue} />
        ))}
      </div>

      {/* Per-band gain-reduction meter region. */}
      <div class="meters">
        <div class="flex items-center justify-between">
          <span class={`${ctrlName} ctrlName`}>Gain reduction (GR)</span>
        </div>
        <div class="meter-grid grid grid-cols-2 gap-3 sm:grid-cols-4">
          {meters.bands.map((m) => (
            <MultibandMeter key={m.band} band={m.band} grDb={m.gr_db} />
          ))}
          {meters.bands.length === 0 && (
            <div class={`${emptyNote} editor-empty emptyNote`}>
              Select a multiband block to see per-band gain-reduction meters.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// --- Global segmented control (bands) -------------------------------------------

/** A small segmented row for the integer-only `bands` param. */
function BandsControl({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  const options = [2, 3, 4] as const;
  return (
    <div class="band-control grid items-center justify-items-center gap-1 w-full col-span-2">
      <span class={`${ctrlName} ctrlName`}>Bands</span>
      <div class="segmented flex border border-line rounded-lg overflow-hidden bg-well" role="radiogroup" aria-label="Number of bands">
        {options.map((opt) => (
          <button
            key={opt}
            class={`seg flex-1 min-h-[var(--ui-touch)] min-w-[var(--ui-touch)] px-3 text-[10px] font-semibold text-muted border-r border-line-soft last:border-r-0 whitespace-nowrap hover:text-text hover:bg-[#1e2027] ${opt === value ? "on bg-linear-to-b from-amber-hi to-amber-lo border-amber-edge text-amber-ink" : ""}`}
            role="radio"
            aria-checked={opt === value}
            onClick={() => onChange(opt)}
            title={`Bands: ${opt}`}
          >
            {opt}
          </button>
        ))}
      </div>
    </div>
  );
}

// --- Per-band strip helpers -----------------------------------------------------

/** A knob for a single global parameter, formatted by its unit. */
function ParamKnob({
  id,
  label,
  value,
  onChange,
}: {
  id: ParamId;
  label: string;
  value: number;
  onChange: (v: number) => void;
}) {
  const p = ALL_PARAMS.find((s) => s.id === id)!;
  return (
    <div class="knob-unit grid items-center justify-items-center gap-1 w-full">
      <span class={`${ctrlName} ctrlName`}>{label}</span>
      <Knob
        name={p.name}
        value={value}
        min={p.min}
        max={p.max}
        step={p.step}
        defaultValue={p.default}
        unit={p.unit}
        format={(v) => formatByUnit(p.unit, v)}
        onChange={onChange}
      />
    </div>
  );
}

/** A marker on the crossover axis: positioned along a log scale at `value`,
 *  with a label showing the frequency. */
function Marker({ value, label }: { value: number; label: string }) {
  // log position of `value` (Hz) in [0, 240] px on a log axis 20 Hz -> 20 kHz.
  const logPos = (v: number): number => 240 * Math.log(v / 20) / Math.log(20000 / 20);
  return (
    <div
      class="crossover-marker absolute flex flex-col items-center"
      style={{ left: `${logPos(value)}px`, top: 0, bottom: 0 }}
    >
      <span class="crossover-marker-tick w-1.5 h-full min-w-[2px] bg-amber" />
      <span class="crossover-marker-label text-[9px] font-mono text-amber-ink">
        {label}
      </span>
    </div>
  );
}
