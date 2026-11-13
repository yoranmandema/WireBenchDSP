import { Knob } from "./Knob";
import { selected, setParam, state } from "../state";
import type { BlockSpec, ParamSpec } from "../protocol";

/** Format a parameter value with the unit the engine declares. */
function formatParam(p: ParamSpec, v: number): string {
  if (p.kind === "choice") return p.choices?.[Math.round(v)] ?? String(v);
  const decimals = Math.abs(v) >= 100 ? 0 : Math.abs(v) >= 10 ? 1 : 2;
  return `${v.toFixed(decimals)}${p.unit ? ` ${p.unit}` : ""}`;
}

/** The bottom panel: the selected block's parameters as knobs. */
export function Editor() {
  const s = state.value;
  const sel = selected.value;
  if (!s) return null;

  if (!sel) {
    return (
      <footer class="editor">
        <div class="editor-empty">Select a block to edit its parameters.</div>
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

  return (
    <footer class="editor">
      <div class="editor-id">
        <div class="editor-kind">Selected block</div>
        <div class="editor-name">
          {block.name}
          {block.bypass && <span class="editor-bypass">bypassed</span>}
        </div>
        <div class="editor-type">{block.type}</div>
      </div>

      <div class="knobs">
        {spec?.params.map((p) => (
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
        )) ?? <div class="editor-empty">This block has no parameters.</div>}
      </div>
    </footer>
  );
}
