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
      <label class="select-unit grid items-center justify-items-center gap-1 w-full col-span-2">
        <span class="ctrlName">{p.name}</span>
        <select class="textField min-h-[var(--ui-touch)] min-w-[var(--ui-touch)] px-3" value={current} onChange={(e) => onChange(Number((e.target as HTMLSelectElement).value))}>
          {choices.map((c, i) => (
            <option key={c} value={i}>{c}</option>
          ))}
        </select>
      </label>
    );
  }

  return (
    <div class="choice-unit grid items-center justify-items-center gap-1 w-full col-span-2">
      <span class="ctrlName">{p.name}</span>
      <div class="segmented flex border border-line rounded-lg overflow-hidden bg-well" role="radiogroup" aria-label={p.name}>
        {choices.map((c, i) => (
          <button
            key={c}
            class={`seg flex-1 min-h-[var(--ui-touch)] min-w-[var(--ui-touch)] px-3 text-[10px] font-semibold text-muted border-r border-line-soft last:border-r-0 whitespace-nowrap hover:text-text hover:bg-[#1e2027] ${i === current ? "bg-linear-to-b from-amber-hi to-amber-lo border-amber-edge text-amber-ink" : ""}`}
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
      <footer class="editor bg-linear-to-b from-[#1a1b21] to-[#14151a] border-t border-line grid items-center">
        <div class="emptyNote">Select a block in the chain to edit it.</div>
      </footer>
    );
  }

  const channel = s.channels.find((c) => c.id === sel.channel);
  const block = channel?.blocks.find((b) => b.id === sel.block);
  if (!block) {
    return (
      <footer class="editor bg-linear-to-b from-[#1a1b21] to-[#14151a] border-t border-line grid items-center">
        <div class="emptyNote">That block is gone.</div>
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
      <div class="editor-id grid items-center justify-items-start gap-1 pl-3 min-h-0" style={spec ? catStyle(spec.category) : undefined}>
        <div class="editor-kind flex items-center gap-1 text-[9px] tracking-[1.4px] font-semibold text-cat">
          {spec && <Icon category={spec.category} size={13} />}
          <span>Selected block</span>
        </div>
        <div class="editor-name flex items-center gap-2 min-w-0 max-w-full">
          <span class="min-w-0 truncate text-xl font-semibold">{block.name}</span>
          {block.bypass && <span class="editor-bypass flex-none px-2 text-[9px] text-amber border border-amber-dim rounded-sm font-medium">bypassed</span>}
        </div>
        <div class="editor-type text-[9px] text-muted font-mono">{block.type}</div>
        <div class="editor-side flex gap-1 mt-0">
          <button class={`sideBtn min-h-[var(--ui-touch)] min-w-[var(--ui-touch)] px-3 rounded-md text-[11px] font-semibold bg-raised border border-line text-muted hover:text-text hover:border-line-hi ${block.bypass ? "text-amber border-amber-dim bg-[#2a2318]" : ""}`} onClick={() => void setBypass(sel.channel, sel.block, !block.bypass)} title="Bypass this block">
            {block.bypass ? "Bypassed" : "Bypass"}
          </button>
          <button class="sideBtn min-h-[var(--ui-touch)] min-w-[var(--ui-touch)] px-3 rounded-md text-[11px] font-semibold bg-raised border border-line text-muted hover:text-text hover:border-line-hi" onClick={resetAll} title="Reset every parameter to its default">
            Reset
          </button>
        </div>
      </div>

      <div class="editor-controls grid gap-y-4 gap-x-6 min-w-0 w-full max-w-[var(--ui-ctrl-row)] mx-auto" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(var(--ui-ctrl), 1fr))", justifyContent: "center", alignContent: "center" }}>
        {params.length === 0 && <div class="emptyNote">This block has no parameters.</div>}
        {choices.map((p) => (
          <ChoiceControl key={p.id} p={p} value={block.params[p.id] ?? p.default} onChange={(v) => void setParam(sel.channel, sel.block, p.id, v)} />
        ))}
        {knobs.map((p) => (
          <Knob key={p.id} name={p.name} value={block.params[p.id] ?? p.default} min={p.min} max={p.max} defaultValue={p.default} unit={p.unit} format={(v) => formatParam(p, v)} onChange={(v) => void setParam(sel.channel, sel.block, p.id, v)} />
        ))}
      </div>
    </>
  );

  return <footer class="editor bg-linear-to-b from-[#1a1b21] to-[#14151a] border-t border-line grid items-center gap-3 p-4" style={{ gridTemplateColumns: "minmax(0, var(--ui-id-w)) minmax(0, 1fr)", minHeight: 0 }}>{body}</footer>;
}