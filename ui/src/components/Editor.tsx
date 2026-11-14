import type { JSX } from "preact";
import { Icon } from "./Icon";
import { Knob } from "./Knob";
import { catStyle } from "../theme";
import { selected, setBypass, setParam, state } from "../state";
import type { BlockSpec, ParamSpec } from "../protocol";

/** Format a parameter value with the unit the engine declares. */
function formatParam(p: ParamSpec, v: number): string {
  if (p.kind === "choice") return p.choices?.[Math.round(v)] ?? String(v);
  const decimals = Math.abs(v) >= 100 ? 0 : Math.abs(v) >= 10 ? 1 : 2;
  return `${v.toFixed(decimals)}${p.unit ? ` ${p.unit}` : ""}`;
}

/**
 * A `choice` parameter. A knob is the wrong control for a small set of named
 * options, so these render as a segmented row (or a dropdown when there are
 * too many to lay out).
 */
function ChoiceControl({
  p,
  value,
  onChange,
}: {
  p: ParamSpec;
  value: number;
  onChange: (v: number) => void;
}) {
  const choices = p.choices ?? [];
  const current = Math.min(choices.length - 1, Math.max(0, Math.round(value)));

  if (choices.length === 0) return null;

  if (choices.length > 5) {
    return (
      <label class="select-unit">
        <span class="knob-name">{p.name}</span>
        <select
          class="select"
          value={current}
          onChange={(e) => onChange(Number((e.target as HTMLSelectElement).value))}
        >
          {choices.map((c, i) => (
            <option key={c} value={i}>
              {c}
            </option>
          ))}
        </select>
      </label>
    );
  }

  return (
    <div class="choice-unit">
      <span class="knob-name">{p.name}</span>
      <div class="segmented" role="radiogroup" aria-label={p.name}>
        {choices.map((c, i) => (
          <button
            key={c}
            class={`seg${i === current ? " on" : ""}`}
            role="radio"
            aria-checked={i === current}
            onClick={() => onChange(i)}
            title={`${p.name}: ${c}`}
          >
            {c}
          </button>
        ))}
      </div>
    </div>
  );
}

/** The bottom panel: the selected block's identity, then its controls. */
export function Editor() {
  const s = state.value;
  const sel = selected.value;
  if (!s) return null;

  if (!sel) {
    return (
      <footer class="editor">
        <div class="editor-empty">Select a block in the chain to edit it.</div>
      </footer>
    );
  }

  const channel = s.channels.find((c) => c.id === sel.channel);
  const block = channel?.blocks.find((b) => b.id === sel.block);
  if (!block) {
    return (
      <footer class="editor">
        <div class="editor-empty">That block is gone.</div>
      </footer>
    );
  }

  const spec: BlockSpec | undefined = s.registry[block.type];
  const params = spec?.params ?? [];
  const knobs = params.filter((p) => p.kind !== "choice");
  const choices = params.filter((p) => p.kind === "choice");

  const resetAll = (): void => {
    for (const p of params) void setParam(sel.channel, sel.block, p.id, p.default);
  };

  const body: JSX.Element = (
    <>
      <div class="editor-id" style={spec ? catStyle(spec.category) : undefined}>
        <div class="editor-kind">
          {spec && <Icon category={spec.category} size={13} />}
          <span>Selected block</span>
        </div>
        <div class="editor-name">
          {block.name}
          {block.bypass && <span class="editor-bypass">bypassed</span>}
        </div>
        <div class="editor-type">{block.type}</div>
        <div class="editor-side">
          <button
            class={`side-btn${block.bypass ? " on" : ""}`}
            onClick={() => void setBypass(sel.channel, sel.block, !block.bypass)}
            title="Bypass this block"
          >
            {block.bypass ? "Bypassed" : "Bypass"}
          </button>
          <button class="side-btn" onClick={resetAll} title="Reset every parameter to its default">
            Reset
          </button>
        </div>
      </div>

      <div class="editor-controls">
        {params.length === 0 && <div class="editor-empty">This block has no parameters.</div>}
        {choices.map((p) => (
          <ChoiceControl
            key={p.id}
            p={p}
            value={block.params[p.id] ?? p.default}
            onChange={(v) => void setParam(sel.channel, sel.block, p.id, v)}
          />
        ))}
        {knobs.map((p) => (
          <Knob
            key={p.id}
            name={p.name}
            value={block.params[p.id] ?? p.default}
            min={p.min}
            max={p.max}
            defaultValue={p.default}
            unit={p.unit}
            format={(v) => formatParam(p, v)}
            onChange={(v) => void setParam(sel.channel, sel.block, p.id, v)}
          />
        ))}
      </div>
    </>
  );

  return <footer class="editor">{body}</footer>;
}
