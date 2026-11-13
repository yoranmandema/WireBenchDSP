import { meters, presetsOpen, state, loadPreset, setSource } from "../state";

const SLOTS = ["A", "B", "C", "D", "E", "F", "G", "H"];

/** Peak meter: fast attack, slow release, drawn as a vertical bar. */
function Meter({ label, value }: { label: string; value: number }) {
  const pct = Math.min(100, value * 100);
  const hot = pct > 70;
  return (
    <div class="meter">
      <div class="meter-track">
        <div class={`meter-fill${hot ? " hot" : ""}`} style={{ height: `${pct}%` }} />
      </div>
      <div class="meter-label">{label}</div>
    </div>
  );
}

export function TopBar() {
  const s = state.value;
  if (!s) return null;
  const idx = s.presets.indexOf(s.preset);

  const step = (d: number) => {
    const next = idx + d;
    if (next >= 0 && next < s.presets.length) void loadPreset(s.presets[next]);
  };

  return (
    <header class="topbar">
      <div class="preset">
        <button class="nav" onClick={() => step(-1)} title="Previous preset">
          ‹
        </button>
        <div class="preset-name">
          {s.preset || "Untitled"}
          <small>{s.source}</small>
        </div>
        <button class="nav" onClick={() => step(1)} title="Next preset">
          ›
        </button>
        <button
          class="nav presets-open"
          onClick={() => (presetsOpen.value = true)}
          title="Browse, save, rename, and delete presets"
        >
          ☰
        </button>
      </div>

      <div class="slots">
        {SLOTS.map((s2, i) => (
          <div
            key={s2}
            class={`slot${idx === i ? " on" : ""}`}
            title={s.presets[i] ?? "empty"}
            onClick={() => s.presets[i] && void loadPreset(s.presets[i])}
          >
            {s2}
          </div>
        ))}
      </div>

      <div class="top-right">
        <div class="source-toggle">
          {(["riff", "live", "silence"] as const).map((src) => (
            <button
              key={src}
              class={`chip${s.source === src ? " on" : ""}`}
              onClick={() => void setSource(src)}
            >
              {src}
            </button>
          ))}
        </div>
        <Meter label="IN" value={meters.value.in} />
        <Meter label="OUT" value={meters.value.out} />
        <div class="cpu" title="DSP load">
          <span class="cpu-value">{meters.value.cpu.toFixed(0)}%</span>
          <span class="cpu-label">DSP</span>
        </div>
      </div>
    </header>
  );
}
