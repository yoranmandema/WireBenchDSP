import { useEffect } from "preact/hooks";
import { error, init, ready } from "./state";
import { TopBar } from "./components/TopBar";
import { Rack } from "./components/Rack";
import { Chain } from "./components/Chain";
import { Editor } from "./components/Editor";

export function App() {
  useEffect(() => {
    void init();
  }, []);

  if (error.value) {
    return (
      <div class="boot">
        <div class="boot-card">
          <h1>Pedal</h1>
          <p>Engine unavailable: {error.value}</p>
          <p class="boot-hint">
            Start the engine with <code>./target/debug/pedal-engine</code> and reload.
          </p>
        </div>
      </div>
    );
  }

  if (!ready.value) {
    return (
      <div class="boot">
        <div class="boot-card">
          <h1>Pedal</h1>
          <p>Connecting to the engine…</p>
        </div>
      </div>
    );
  }

  return (
    <div class="app">
      <TopBar />
      <div class="main">
        <Rack />
        <Chain />
      </div>
      <Editor />
    </div>
  );
}
