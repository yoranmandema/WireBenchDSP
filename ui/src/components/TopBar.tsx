import { meters, presetsOpen, state, loadPreset, setSource, tunerOpen } from "../state";
import { TunerIcon } from "./Icon";

const SLOTS = ["A", "B", "C", "D", "E", "F", "G", "H"];

/** Peak meter: fast attack, slow release, drawn as a vertical bar. */
function Meter({ label, value }: { label: string; value: number }) {
  const pct = Math.min(100, value * 100);
  const hot = pct > 70;
  return (
    <div class="meter flex flex-col items-center gap-1">
      <div class="meter-track w-2 h-[22px] bg-inset border border-line-soft rounded-sm overflow-hidden flex items-end">
        <div
          class={`meter-fill w-full transition-[height] duration-60 ease-linear ${hot ? "bg-amber" : "bg-ok"}`}
          style={{ height: `${pct}%` }}
        />
      </div>
      <div class="meter-label text-[9px] tracking-[0.6px] text-muted">{label}</div>
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
    <header class="topbar flex min-h-[var(--ui-topbar-h)] items-center gap-4.5 px-4 min-w-0 bg-linear-to-b from-[#1e2027] to-[#16171c] border-b border-line compact:flex-wrap compact:gap-y-2 compact:px-3 compact:py-2">
      <div class="preset flex items-center gap-2 min-w-[230px] compact:flex-1 compact:min-w-0">
        <button class="nav size-6.5 rounded-key border border-line bg-raised-2 leading-none text-muted hover:border-line-hi hover:text-text" onClick={() => step(-1)} title="Previous preset">
          ‹
        </button>
        <div class="preset-name flex flex-col min-w-0">
          <span class="text-lg font-semibold truncate">{s.preset || "Untitled"}</span>
          <small class="text-[10px] font-normal text-muted capitalize">{s.source}</small>
        </div>
        <button class="nav size-6.5 rounded-key border border-line bg-raised-2 leading-none text-muted hover:border-line-hi hover:text-text" onClick={() => step(1)} title="Next preset">
          ›
        </button>
        <button class="nav presets-open size-6.5 rounded-key border border-line bg-raised-2 leading-none text-muted hover:border-line-hi hover:text-text" onClick={() => (presetsOpen.value = true)} title="Browse, save, rename, and delete presets">
          ☰
        </button>
      </div>

      <div class="slots flex gap-1">
        {SLOTS.map((s2, i) => (
          <div
            key={s2}
            class={`slot size-6.5 rounded-key bg-[#22242b] border border-line-soft text-muted text-[11px] font-semibold grid place-items-center ${idx === i ? "on bg-linear-to-b from-amber-hi to-amber-lo border-amber-edge text-amber-ink shadow-glow" : ""}`}
            title={s.presets[i] ?? "empty"}
            onClick={() => s.presets[i] && void loadPreset(s.presets[i])}
          >
            {s2}
          </div>
        ))}
      </div>

      <div class="top-right flex items-center gap-3.5 ml-auto compact:flex-wrap compact:gap-y-2">
        <button
          class={`min-h-[var(--ui-touch)] px-2.5 flex items-center gap-1.5 rounded-md border text-[10px] font-semibold tracking-[0.6px] uppercase ${tunerOpen.value ? "on bg-[#2f323b] text-text border-line inset-shadow-[0_-2px_0_var(--color-amber)]" : "bg-[#1a1b21] text-muted border-line hover:text-text hover:border-line-hi"}`}
          onClick={() => (tunerOpen.value = true)}
          title="Tuner (T)"
          aria-label="Open the tuner"
          aria-expanded={tunerOpen.value}
        >
          <TunerIcon size={11} />
          <span>Tuner</span>
        </button>
        <div class="source-toggle flex border border-line rounded-md overflow-hidden">
          {(["riff", "live", "silence"] as const).map((src) => (
            <button
              key={src}
              class={`chip px-2.5 py-1.25 text-[10px] font-semibold tracking-[0.6px] uppercase bg-[#1a1b21] text-muted border-r border-line-soft last:border-r-0 ${s.source === src ? "on bg-[#2f323b] text-text inset-shadow-[0_-2px_0_var(--color-amber)]" : ""}`}
              onClick={() => void setSource(src)}
            >
              {src}
            </button>
          ))}
        </div>
        <Meter label="IN" value={meters.value.in} />
        <Meter label="OUT" value={meters.value.out} />
        <div class="cpu grid place-items-center gap-1" title="DSP load">
          <span class="cpu-value text-[11px] tabular-nums text-text">{meters.value.cpu.toFixed(0)}%</span>
          <span class="cpu-label text-[9px] tracking-[0.6px] text-muted">DSP</span>
        </div>
      </div>
    </header>
  );
}