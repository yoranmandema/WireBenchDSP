// Seeded engine state for the Cypress E2E suite.
//
// The fake engine (fake-engine.mjs) is a stand-in for `pedal-engine`: it speaks
// the same WebSocket protocol and serves this state, so the UI runs exactly as
// it does against the real thing but with a rig that is identical on every
// test. The registry mirrors the real `builtin_registry()` block specs (names,
// categories, parameter ranges and units) so the UI renders the same controls.
//
// Two entries are deliberate test hooks, not real engine behaviour:
//   - the `locked` block (ch1) always rejects `removeBlock`, so the command
//     failure path (toast) is reachable;
//   - the `02-lead` preset always rejects `deletePreset`, so the preset
//     browser's error state is reachable.
// `util.voices` is a fixture-only block: the real registry has no choice
// parameter with more than five options, and the editor renders those as a
// `<select>` rather than a segmented row. It exists to exercise that branch.

/** A number parameter. */
function num(id, name, min, max, defaultValue, unit = "") {
  return { id, name, kind: "number", min, max, default: defaultValue, unit };
}

/** A choice parameter. */
function choice(id, name, choices, defaultValue) {
  return { id, name, kind: "choice", min: 0, max: choices.length - 1, default: defaultValue, unit: "", choices };
}

export function seedRegistry() {
  return {
    "amp.basic": {
      name: "Amp",
      category: "Modelling",
      params: [
        choice("voice", "Voice", ["Clean", "Crunch", "Lead"], 1),
        num("gain", "Gain", 0, 10, 5),
        num("bass", "Bass", -12, 12, 0, "dB"),
        num("mid", "Mid", -12, 12, 0, "dB"),
        num("treble", "Treble", -12, 12, 0, "dB"),
        num("level", "Level", -40, 12, -6, "dB"),
      ],
    },
    "cab.sim": {
      name: "Cab",
      category: "Modelling",
      params: [
        num("low_cut", "Low cut", 40, 200, 80, "Hz"),
        num("resonance", "Resonance", 0, 12, 4, "dB"),
        num("presence", "Presence", -9, 9, 2, "dB"),
        num("high_cut", "High cut", 2000, 10000, 5000, "Hz"),
      ],
    },
    "mod.nam": {
      name: "NAM",
      category: "Modelling",
      params: [
        num("gain", "Input gain", -24, 24, 0, "dB"),
        num("level", "Output level", -30, 12, 0, "dB"),
        num("mix", "Mix", 0, 100, 100, "%"),
      ],
    },
    "dist.overdrive": {
      name: "Overdrive",
      category: "Distortion",
      params: [num("drive", "Drive", 0, 10, 1), num("level", "Level", -30, 12, 0, "dB")],
    },
    "tone.stack": {
      name: "Tone Stack",
      category: "Tone",
      params: [
        num("low", "Low", -20, 20, 0, "dB"),
        num("mid", "Mid", -20, 20, 0, "dB"),
        num("high", "High", -20, 20, 0, "dB"),
      ],
    },
    "time.delay": {
      name: "Delay",
      category: "Time",
      params: [
        num("time", "Time", 20, 1000, 300, "ms"),
        num("feedback", "Feedback", 0, 95, 35, "%"),
        num("mix", "Mix", 0, 100, 30, "%"),
      ],
    },
    "time.reverb": {
      name: "Reverb",
      category: "Time",
      params: [num("mix", "Mix", 0, 100, 30, "%")],
    },
    "time.pitchdelay": {
      name: "Pitch Delay",
      category: "Pitch",
      params: [
        num("time", "Time", 20, 1000, 380, "ms"),
        num("feedback", "Feedback", 0, 95, 55, "%"),
        num("pitch", "Pitch", -1200, 1200, 700, "cents"),
        num("rate", "Sweep rate", -1200, 1200, 0, "cents"),
        choice("mode", "Mode", ["Target", "Sweep", "Shepard"], 0),
        num("mix", "Mix", 0, 100, 40, "%"),
      ],
    },
    "util.gain": {
      name: "Gain",
      category: "Utility",
      params: [num("gain", "Gain", -60, 24, 0, "dB")],
    },
    "util.master": {
      name: "Master",
      category: "Utility",
      params: [num("volume", "Volume", -60, 12, 0, "dB"), num("ceiling", "Ceiling", -24, 0, -1, "dB")],
    },
    // Fixture-only: six options force the editor's `<select>` branch.
    "util.voices": {
      name: "Voices",
      category: "Utility",
      params: [choice("voice", "Voice", ["Clean", "Crunch", "Lead", "Rhythm", "Acoustic", "Custom"], 0)],
    },
  };
}

/** Default parameter values for a registry spec. */
export function defaultParams(spec) {
  return Object.fromEntries(spec.params.map((p) => [p.id, p.default]));
}

/** A block instance. */
function block(id, type, name, params = {}, bypass = false) {
  return { id, type, name, bypass, params };
}

/**
 * A fresh snapshot. Called by the fake engine on every connection so each
 * Cypress test starts from the same rig.
 */
export function createSeedState() {
  const registry = seedRegistry();
  const spec = (type) => registry[type];

  return {
    rev: 0,
    preset: "01-clean",
    presets: ["00-blank", "01-clean", "02-lead"],
    inputGain: 0,
    masterOutVolume: 0,
    source: "riff",
    registry,
    routings: [],
    channels: [
      {
        id: "ch1",
        name: "Channel 1",
        volume: -3,
        sources: ["<in>"],
        blocks: [
          block("amp", "amp.basic", "Amp", defaultParams(spec("amp.basic"))),
          block("delay", "time.delay", "Delay", defaultParams(spec("time.delay"))),
          // Test hook: `removeBlock` for this id always fails.
          block("locked", "time.reverb", "Reverb", defaultParams(spec("time.reverb")), true),
        ],
      },
      {
        id: "ch2",
        name: "Channel 2",
        volume: 0,
        sources: ["<in>"],
        blocks: [
          block("drive", "dist.overdrive", "Overdrive", defaultParams(spec("dist.overdrive"))),
          block("voices", "util.voices", "Voices", defaultParams(spec("util.voices"))),
        ],
      },
      { id: "ch3", name: "Channel 3", volume: 0, sources: [], blocks: [] },
      { id: "ch4", name: "Channel 4", volume: 0, sources: [], blocks: [] },
    ],
  };
}

export const METERS = { in: 0.42, out: 0.38, cpu: 12.3, xruns: 0 };
