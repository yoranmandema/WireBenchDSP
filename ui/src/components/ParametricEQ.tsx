import { useCallback } from "preact/hooks";
import { CurveChart } from "./CurveChart";
import {
  bandControls,
  eqCurve,
  type EQBand,
} from "../blocks/parametric-eq";
import { setParam, state, selected } from "../state";
import type { BlockSpec, ParamSpec } from "../protocol";
import { ctrlName } from "../ui";
import "./parametric-eq.css";

// --- Real slider (used in place of the SVG knob) --------------------------
// Renders a native `<input type="range">` with a label, unit and value
// readback. Native range inputs are keyboard-operable (Tab to focus, ArrowUp/
// ArrowDown / ArrowLeft / ArrowRight to step), which satisfies the a11y
// requirement without a custom pointer-drag implementation.
//
// The `log` flag enables a logarithmic travel: equal pointer travel maps to a
// constant ratio instead of a constant step. This is used for the frequency
// controls (Hz), which is what the design doc (§3, curve = Log) requires.
// The input's min/max/value are stored in log space; the readback is always in
// the user's unit (Hz, dB, …).

export interface SliderProps {
  name: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  log?: boolean;
  format?: (v: number) => string;
  onChange: (v: number) => void;
}

function Slider({
  name,
  value,
  min,
  max,
  step = 1,
  unit,
  log = false,
  format,
  onChange,
}: SliderProps) {
  const display =
    format !== undefined
      ? format(value)
      : `${value.toFixed(2)}${unit ? ` ${unit}` : ""}`;

  const handleInput = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const raw = parseFloat(input.value);
    const v = log ? Math.exp(raw) : raw;
    onChange(v);
  };

  const inputMin = log ? Math.log(min) : min;
  const inputMax = log ? Math.log(max) : max;
  const inputValue = log ? Math.log(value) : value;

  return (
    <div class="slider-unit">
      <div class="slider-label">
        <span class="slider-name">{name}</span>
        {unit ? <span class="slider-unit">{unit}</span> : null}
      </div>
      <input
        class="slider-input"
        type="range"
        min={inputMin}
        max={inputMax}
        step={log ? 0.01 : step}
        value={inputValue}
        aria-label={`${name}${unit ? ` ${unit}` : ""}`}
        title={`${name}: ${display}`}
        onInput={handleInput}
      />
      <div class="slider-readout">{display}</div>
    </div>
  );
}

// --- Band row -------------------------------------------------------------
// One row per band: an enable toggle plus a frequency slider, and a gain and Q
// slider where the band has those parameters (design §2).

interface BandRowProps {
  band: EQBand;
  spec: BlockSpec;
  value: (id: string) => number;
  onChange: (id: string, v: number) => void;
}

function BandRow({ band, spec, value, onChange }: BandRowProps) {
  const p = (id: string): ParamSpec =>
    spec.params.find((x) => x.id === id)!;
  const v = (id: string): number => value(id);

  const gainId = band.gainId;
  const qId = band.qId;

  return (
    <div class="eq-band">
      <div class="eq-band-head">
        <span class="eq-band-name">{band.name}</span>
        <label
          class="eq-toggle"
          title={`${band.name} on/off`}
          aria-label={`${band.name} ${v(band.enabledId) === 0 ? "disabled" : "enabled"}`}
        >
          <input
            type="checkbox"
            class="eq-toggle-input"
            checked={v(band.enabledId) !== 0}
            aria-checked={v(band.enabledId) !== 0}
            title={`${band.name}: ${v(band.enabledId) === 0 ? "off" : "on"}`}
            onKeyDown={(e) => {
              if (e.key === " " || e.key === "Enter") {
                e.preventDefault();
                onChange(band.enabledId, v(band.enabledId) === 0 ? 1 : 0);
              }
            }}
            tabIndex={0}
          />
          <span class="eq-toggle-track"></span>
          <span class="eq-toggle-status">{v(band.enabledId) === 0 ? "Off" : "On"}</span>
        </label>
      </div>
      <div class="eq-band-controls">
        {/* Frequency slider (log). */}
        <div class="eq-cell">
          <Slider
            name={`${band.name} freq`}
            value={v(band.freqId)}
            min={p(band.freqId).min}
            max={p(band.freqId).max}
            step={1}
            unit="Hz"
            log
            format={(n) => {
              const a = Math.abs(n);
              if (a >= 1000) return `${(n / 1000).toFixed(1)} kHz`;
              if (a >= 100) return `${n.toFixed(0)} Hz`;
              if (a >= 10) return `${n.toFixed(1)} Hz`;
              return `${n.toFixed(2)} Hz`;
            }}
            onChange={(nv) => onChange(band.freqId, nv)}
          />
        </div>
        {/* Gain slider (dB). */}
        {gainId && (
          <div class="eq-cell">
            <Slider
              name={`${band.name} gain`}
              value={v(gainId)}
              min={p(gainId).min}
              max={p(gainId).max}
              step={0.1}
              unit="dB"
              format={(n) => `${n >= 0 ? "+" : ""}${n.toFixed(2)} dB`}
              onChange={(nv) => onChange(gainId, nv)}
            />
          </div>
        )}
        {/* Q slider (peaking bands only). */}
        {qId && (
          <div class="eq-cell">
            <Slider
              name="Q"
              value={v(qId)}
              min={p(qId).min}
              max={p(qId).max}
              step={0.01}
              format={(n) => `${n.toFixed(2)}`}
              onChange={(nv) => onChange(qId, nv)}
            />
          </div>
        )}
      </div>
    </div>
  );
}

/** Fixed Q values (design §3): shelves 0.707, HP 0.707, LP 0.54. */
export const FIXED_Q: Record<string, number> = {
  low_shelf: 0.707,
  low_peak: 1.0,
  high_peak: 1.0,
  high_shelf: 0.707,
  high_pass: 0.707,
  low_pass: 0.54,
};

/** The parametric EQ block panel. */
export function ParametricEQ() {
  const s = state.value;
  const sel = selected.value;
  if (!s || !sel) return null;

  const channel = s.channels.find((c) => c.id === sel.channel);
  const block = channel?.blocks.find((b) => b.id === sel.block);
  if (!block) return null;

  const spec: BlockSpec | undefined = s.registry[block.type];
  if (!spec) return null;

  const bands = bandControls(spec);
  const value = useCallback(
    (id: string): number => block.params[id] ?? spec.params.find((p) => p.id === id)!.default,
    [block, spec],
  );

  const onChange = useCallback(
    (id: string, v: number) => {
      void setParam(sel.channel, sel.block, id, v);
    },
    [sel.channel, sel.block],
  );

  const curve = eqCurve(block.params);

  return (
    <div class="eq-panel">
      <div class="eq-identity">
        <div class="eq-identity-name">Parametric EQ</div>
        <div class="eq-identity-type">tone.eq</div>
      </div>

      <div class="eq-body">
        {bands.map((band) => (
          <BandRow key={band.id} band={band} spec={spec} value={value} onChange={onChange} />
        ))}

        {/* Global output gain, applied after the band chain (design §2). */}
        <div class="eq-global">
          <div class={`${ctrlName} ctrlName`}>Output gain</div>
          <Slider
            name="Output gain"
            value={value("output_gain")}
            min={-12}
            max={12}
            step={0.1}
            unit="dB"
            format={(n) => `${n >= 0 ? "+" : ""}${n.toFixed(2)} dB`}
            onChange={(nv) => onChange("output_gain", nv)}
          />
        </div>

        {/* Frequency-response curve: log x, dB y, 0..±24 dB, one trace per
            band; flat for a disabled or 0-gain band (design §1). */}
        <div class="eq-curve-wrap">
          <CurveChart bands={curve} caption="Parametric EQ frequency response" />
        </div>
      </div>
    </div>
  );
}
