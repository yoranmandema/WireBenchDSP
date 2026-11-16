import { useEffect } from "preact/hooks";
import { error, init, notice, ready } from "./state";
import { TopBar } from "./components/TopBar";
import { Rack } from "./components/Rack";
import { Chain } from "./components/Chain";
import { Editor } from "./components/Editor";
import { PresetBrowser } from "./components/PresetBrowser";
import { AddBlockModal } from "./components/AddBlockModal";
import { SelectionRing } from "./selection";

export function App() {
  useEffect(() => {
    void init();
  }, []);

  if (error.value) {
    return (
      <div class="boot grid place-items-center h-full bg-stage">
        <div class="boot-card text-center grid gap-2">
          <h1 class="text-2xl tracking-[6px] font-bold">Pedal</h1>
          <p>Engine unavailable: {error.value}</p>
          <p class="boot-hint text-muted">
            Start the engine with <code class="px-1.5 py-0.5 rounded-sm bg-[#121318] border border-line-soft text-xs">./target/debug/pedal-engine</code> and reload.
          </p>
        </div>
      </div>
    );
  }

  if (!ready.value) {
    return (
      <div class="boot grid place-items-center h-full bg-stage">
        <div class="boot-card text-center grid gap-2">
          <h1 class="text-2xl tracking-[6px] font-bold">Pedal</h1>
          <p>Connecting to the engine…</p>
        </div>
      </div>
    );
  }

  return (
    <div class="app grid h-full" style={{ gridTemplateRows: "auto minmax(0, 1fr) auto" }}>
      <TopBar />
      <div class="main grid min-h-0" style={{ gridTemplateColumns: "240px 1fr" }}>
        <Rack />
        <Chain />
      </div>
      <Editor />
      <PresetBrowser />
      <AddBlockModal />
      {/* Drawn above the chain but outside every scrolling container, so the
          ring is never clipped by the strip or the panel. */}
      <SelectionRing />
      {/* Command failures surface here instead of failing silently. */}
      {notice.value && (
        <div class="toast fixed z-50 rounded-lg bg-danger-bg border border-danger-line text-danger-text text-base cursor-pointer shadow-toast" onClick={() => (notice.value = null)}>
          {notice.value}
        </div>
      )}
    </div>
  );
}