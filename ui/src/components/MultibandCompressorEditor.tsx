// The multiband compressor's control panel (renderer + binding).
//
// A single block in the channel strip, exactly like `dynamics.compressor`. It
// renders the global controls (bands, crossover frequencies, output gain,
// dry/wet), one column of per-band controls (threshold / ratio / attack /
// release / makeup plus bypass / solo) per active band, and a per-band
// gain-reduction meter region.
//
// Binding: every control reads from a `ParamSource` and commits through
// `setValue` — the same seam every other editor uses via `setParam`. With no
// engine present the panel is driven by `LocalParamSource` / `LocalMetersSource`
// (in-memory stubs), so it renders and can be tested headless. Connecting the
// engine is a one-line swap: pass an `EngineParamSource` that reads the rig
// snapshot and writes the `setParam` command.

import { useMemo } from "preact/hooks";
import type { JSX } from "preact";
import { makeLogScale } from "../graph/scale";
import type { Block, BlockSpec } from "../protocol";
import { ctrlName, emptyNote } from "../ui";
import { Knob } from "./Knob";
import { Format, ALL_PARAMS, type ParamId } from "../params";
import {
  LocalParamSource,
  useMultibandParams,
  LocalMetersSource,
  useMultibandMeters,
} from "../hooks/useParams";

// The seven per-band parameter keys in listing order.
const BAND_KEYS = ["threshold", "ratio", "attack", "release", "makeup", "bypass", "solo"] as const;
type BandKey = (typeof BAND_KEYS)[number];

/** A per-band parameter id, built type-safely from a band index and a key. */
const bandParam = (band: number, key: BandKey): ParamId => `band_${band}_${key}` as ParamId;

/** Props shared with the block editors (channel / block / spec). */
export function MultibandCompressorEditor({
  _channel,
  block,
  spec,
}: {
  _channel: string;
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
  const source = useMemo(() => new LocalParamSource(params), [params]);
  const { values, setValue } = useMultibandParams(source, block);
  const meters = useMultibandMeters(new LocalMetersSource());

  const g = (id: ParamId): number => values[id];

  return (
    <div class="multiband-editor flex flex-col w-full min-w-0 gap-3">
      {/* Identity banner: block name, type, bypass, reset. */}
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

      {/* Crossover axis: a log-scale axis with the three crossover markers. */}
      <div class="crossover-axis">
        <div
          class="crossover-track relative"
          style={{ height: 28, padding: "0 2px", overflow: "hidden" }}
        >
          {(["crossover_1", "crossover_2", "crossover_3"] as const).map((id) => (
            <Marker
              key={id}
              value={g(id)}
              label={`${id}: ${Format.hz(g(id))}`}
            />
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

      {/* Per-band parameter columns: threshold / ratio / attack / release /
       * makeup plus bypass / solo toggles. */}
      <div class="bands">
        {activeBands.map((band) => (
          <BandColumn
            key={band}
            band={band}
            values={values}
            onParamChange={setValue}
          />
        ))}
      </div>

      {/* Per-band gain-reduction meters (placeholder regions). */}
      <div class="meters">
        <div class="flex items-center justify-between">
          <span class={`${ctrlName} ctrlName`}>Gain reduction (GR)</span>
        </div>
        <div class="meter-grid grid grid-cols-2 gap-3 sm:grid-cols-4">
          {meters.bands.map((m) => (
            <MeterMark key={m.band} band={m.band} grDb={m.gr_db} />
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
function BandsControl({
  value,
  onChange,
}: {
  value: number;
  onChange: (v: number) => void;
}) {
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

// --- Per-band column ---------------------------------------------------------------

/** One band's column: a header plus the seven control cells. */
function BandColumn({
  band,
  values,
  onParamChange,
}: {
  band: number;
  values: Readonly<Record<ParamId, number>>;
  onParamChange: (id: ParamId, v: number) => void;
}) {
  const header = (
    <div class="flex items-center justify-between mb-1">
      <span class={`${ctrlName} ctrlName`}>Band {band}</span>
      <span class="text-[10px] text-muted tabular-nums">
        {Format.db(values[bandParam(band, "threshold")])}
      </span>
    </div>
  );

  return (
    <div class="band-column editor-controls grid grid-cols-[repeat(auto-fit,minmax(var(--ui-ctrl),1fr))] content-center justify-center gap-x-6 gap-y-4 min-w-0 w-full">
      {header}
      <ParamKnob
        id={bandParam(band, "threshold")}
        label="Threshold"
        value={values[bandParam(band, "threshold")]}
        onChange={(v) => onParamChange(bandParam(band, "threshold"), v)}
      />
      <ParamKnob
        id={bandParam(band, "ratio")}
        label="Ratio"
        value={values[bandParam(band, "ratio")]}
        onChange={(v) => onParamChange(bandParam(band, "ratio"), v)}
      />
      <ParamKnob
        id={bandParam(band, "attack")}
        label="Attack"
        value={values[bandParam(band, "attack")]}
        onChange={(v) => onParamChange(bandParam(band, "attack"), v)}
      />
      <ParamKnob
        id={bandParam(band, "release")}
        label="Release"
        value={values[bandParam(band, "release")]}
        onChange={(v) => onParamChange(bandParam(band, "release"), v)}
      />
      <ParamKnob
        id={bandParam(band, "makeup")}
        label="Makeup"
        value={values[bandParam(band, "makeup")]}
        onChange={(v) => onParamChange(bandParam(band, "makeup"), v)}
      />
      <div class="flex gap-3">
        <ToggleParam
          id={bandParam(band, "bypass")}
          label="Bypass"
          onCommit={(v) => onParamChange(bandParam(band, "bypass"), v)}
        />
        <ToggleParam
          id={bandParam(band, "solo")}
          label="Solo"
          onCommit={(v) => onParamChange(bandParam(band, "solo"), v)}
        />
      </div>
    </div>
  );
}

// --- Reusable knob / toggle controls -------------------------------------------------

/** A knob for a single parameter, formatted by its unit. */
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
        defaultValue={p.default}
        unit={p.unit}
        format={(v) => formatByUnit(p.unit, v)}
        onChange={onChange}
      />
    </div>
  );
}

/** A toggle cell for the 0/1 bypass/solo params. */
function ToggleParam({
  id,
  label,
  onCommit,
}: {
  id: ParamId;
  label: string;
  onCommit: (v: number) => void;
}) {
  const on = values[id] !== 0;
  return (
    <div class="toggle-unit grid items-center justify-items-center gap-1 w-full">
      <span class={`${ctrlName} ctrlName`}>{label}</span>
      <button
        class={`toggle-chip flex-1 min-h-[var(--ui-touch)] min-w-[var(--ui-touch)] px-3 py-2 text-[10px] font-semibold rounded-md border text-muted border-line-soft hover:text-text hover:border-line-hi transition-colors ${on ? "on text-amber border-amber dim bg-[#2a2318] border-amber-edge shadow-[0_0_0_1px_var(--color-amber-dim)]" : ""}`}
        role="switch"
        aria-checked={on}
        aria-label={`${label}: ${on ? "on" : "off"}`}
        title={`${label}: ${on ? "on" : "off"}`}
        onClick={() => onCommit(on ? 0 : 1)}
      >
        {on ? "On" : "Off"}
      </button>
    </div>
  );
}

// --- Meter placeholder ---------------------------------------------------------------

/** A horizontal gain-reduction scale: 0 dB at centre, red below −3 dB. */
function MeterMark({ band, grDb }: { band: string; grDb: number }) {
  const pct = Math.max(0, Math.min(100, (grDb / -3) * 100));
  return (
    <div class="meter flex flex-col items-center gap-1">
      <span class="text-[10px] font-mono text-muted">{band}</span>
      <div
        class="meter-track h-[8px] min-w-0 flex-1 rounded-full bg-well overflow-hidden border border-line-soft"
        role="progressbar"
        aria-valuenow={Math.round(pct)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={`${band} gain reduction`}
      >
        <div
          class="h-full rounded-full bg-red-500 transition-width duration-100"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span class="text-[9px] font-mono text-muted">{Format.db(grDb)}</span>
    </div>
  );
}

// --- Helpers ----------------------------------------------------------------------------

/** Format a value by its schema unit. */
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
 * A marker on the crossover axis: positioned along a log scale at `value`,
 * with a label showing the frequency.
 */
function Marker({ value, label }: { value: number; label: string }) {
  const domain: [number, number] = [20, 20000];
  const range: [number, number] = [0, 240];
  const scale = makeLogScale(domain, range);
  return (
    <div
      class="crossover-marker absolute flex flex-col items-center"
      style={{ left: `${scale.toPixel(value)}px`, top: 0, bottom: 0 }}
    >
      <span class="crossover-marker-tick w-1.5 h-full min-w-[2px] bg-amber" />
      <span class="crossover-marker-label text-[9px] font-mono text-amber-ink">
        {label}
      </span>
    </div>
  );
}
