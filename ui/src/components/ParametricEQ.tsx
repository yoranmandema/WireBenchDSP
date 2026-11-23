import { useCallback } from "preact/hooks";
import { Knob } from "./Knob";
import { FrequencyKnob } from "./FrequencyKnob";
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

// The parametric EQ panel: renders the selected `tone.eq` block's controls from
// the engine's registry snapshot (state.registry), and the curve client-side.
// The band structure and the curve live in `parametric-eq.ts`; the values are
// the block's own params, which the engine drives over the IPC contract.

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
        {/* Frequency knob: log scale (design §3: Hz → Log curve). */}
        <div class="eq-cell">
          <FrequencyKnob
            name={`${band.name} freq`}
            value={v(band.freqId)}
            min={p(band.freqId).min}
            max={p(band.freqId).max}
            defaultValue={p(band.freqId).default}
            unit="Hz"
            format={(n) => `${n.toFixed(0)} Hz`}
            onChange={(nv) => onChange(band.freqId, nv)}
          />
        </div>
        {/* Gain knob (design §3: dB → AmpGain). */}
        {gainId && (
          <div class="eq-cell">
            <Knob
              name={`${band.name} gain`}
              value={v(gainId)}
              min={p(gainId).min}
              max={p(gainId).max}
              defaultValue={p(gainId).default}
              unit="dB"
              format={(n) => `${n.toFixed(2)}${n >= 0 ? " +" : ""} dB`}
              onChange={(nv) => onChange(gainId, nv)}
            />
          </div>
        )}
        {/* Q knob (peaking bands only, design §2). */}
        {qId && (
          <div class="eq-cell">
            <Knob
              name="Q"
              value={v(qId)}
              min={p(qId).min}
              max={p(qId).max}
              defaultValue={p(qId).default}
              unit=""
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

  // Rendered 1:1 from the engine registry (design §5): the UI never ships its
  // own values; it reads the spec for ranges/defaults and the block for the
  // current values.
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
          <Knob
            name="Output gain"
            value={value("output_gain")}
            min={-12}
            max={12}
            defaultValue={0}
            unit="dB"
            format={(n) => `${n.toFixed(2)}${n >= 0 ? " +" : ""} dB`}
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
