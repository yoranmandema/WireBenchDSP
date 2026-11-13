import { state, addBlock, addChannel } from "../state";
import type { Category } from "../protocol";

const ORDER: Category[] = [
  "Modelling",
  "Distortion",
  "Tone",
  "Dynamics",
  "Time",
  "Modulation",
  "Pitch",
  "Utility",
];

/** The left rack: every block type the engine offers, grouped by category. */
export function Rack() {
  const s = state.value;
  if (!s) return null;

  const byCategory = new Map<Category, Array<{ type: string; name: string }>>();
  for (const [type, spec] of Object.entries(s.registry)) {
    const list = byCategory.get(spec.category) ?? [];
    list.push({ type, name: spec.name });
    byCategory.set(spec.category, list);
  }

  return (
    <aside class="rack">
      {ORDER.filter((c) => byCategory.has(c)).map((c) => (
        <section key={c}>
          <h2>{c}</h2>
          {(byCategory.get(c) ?? []).map((b) => (
            <button
              key={b.type}
              class="rack-item"
              onClick={() => {
                const ch = s.channels[0];
                if (ch) void addBlock(ch.id, b.type, b.name);
              }}
              title={`Add ${b.name}`}
            >
              <span class="rack-name">{b.name}</span>
              <span class="rack-type">{b.type}</span>
            </button>
          ))}
        </section>
      ))}
      <section>
        <h2>Channels</h2>
        <button class="rack-item" onClick={() => void addChannel()} title="Add a channel">
          <span class="rack-name">+ Channel</span>
          <span class="rack-type">
            {s.channels.length} of 4
          </span>
        </button>
      </section>
    </aside>
  );
}
