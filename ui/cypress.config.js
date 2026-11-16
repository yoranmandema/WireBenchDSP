import { defineConfig } from "cypress";
import { startFakeEngine, stopFakeEngine } from "./cypress/support/fake-engine.mjs";

// The E2E suite runs against the Vite dev server (`pnpm dev:e2e`), which the
// app talks to over a WebSocket. The server on the other end of that socket is
// the in-process fake engine started below — same protocol, seeded state, no
// audio device. See cypress/support/fake-engine.mjs.
export default defineConfig({
  e2e: {
    baseUrl: "http://localhost:5273",
    specPattern: "cypress/e2e/**/*.cy.js",
    supportFile: "cypress/support/e2e.js",
    fixturesFolder: false,
    video: false,
    screenshotOnRunFailure: false,
    defaultCommandTimeout: 8000,
    requestTimeout: 10000,
    responseTimeout: 10000,
    viewportWidth: 1440,
    viewportHeight: 900,
    env: {
      // Must match VITE_ENGINE_URL in the `dev:e2e` script.
      enginePort: 9400,
    },
    setupNodeEvents(on, config) {
      let engine = null;
      on("before:run", async () => {
        engine = await startFakeEngine({ port: config.env.enginePort });
      });
      on("after:run", async () => {
        await stopFakeEngine(engine);
      });
      return config;
    },
  },
});
