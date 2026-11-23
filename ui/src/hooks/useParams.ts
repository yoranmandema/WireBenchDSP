// Binding layer between the multiband panel and the parameter source.
//
// Every control in the panel reads from a `ParamSource` and commits through
// `setValue`. The panel itself never reaches for `block.params` directly and never
// imports `setParam`, so the same component works against a local stub (no engine
// required) and against the real engine (one swappable source).
//
// The existing UI convention for reading block params is `block.params[id]` with
// a `schema.default` fallback (every editor repeats this), and the mutation path
// is the `setParam(channel, block, param, value)` command on `state.ts`. This hook
// centralises both: resolution from the source, commit through the existing command.
//
// A "mock/stub source of params and meter values" is provided so the panel can be
// built and previewed with no engine running (the acceptance path).

import { useMemo } from "preact/hooks";
import type { ParamId } from "../params";
import type { Block } from "../protocol";

/** Every parameter id the multiband compressor exposes. */
import { ALL_PARAMS } from "../params";

/** A read/write source of parameter values. The UI never talks to the engine
 *  directly; it goes through a source. Swapping the source is the only change
 *  needed to run the panel against the real engine later. */
export interface ParamSource {
  /** Current value; falls back to `defaultValue` when the param is absent. */
  getValue: (id: ParamId, defaultValue: number) => number;
  /** Commit a new value. */
  setValue: (id: ParamId, value: number) => void;
}

/** Local, engine-less source of parameter values.
 *
 * Values live in a plain map and start from the schema defaults. Nothing here
 * talks to a socket or a state signal, so the panel renders and behaves exactly
 * as it will against the engine — the engine binding is a thin wrapper over
 * `ParamSource`.
 */
export class LocalParamSource implements ParamSource {
  private params = new Map<ParamId, number>();

  constructor(initial?: Readonly<Record<ParamId, number>>) {
    if (initial) {
      for (const [id, v] of Object.entries(initial)) {
        if (Object.prototype.hasOwnProperty.call(initial, id) && v !== undefined) {
          this.params.set(id as ParamId, v);
        }
      }
    }
  }

  getValue(id: ParamId, defaultValue: number): number {
    return this.params.has(id) ? this.params.get(id)! : defaultValue;
  }

  setValue(id: ParamId, value: number): void {
    this.params.set(id, value);
  }

  /** Seed every param from the engine's default map (`spec.default`). */
  seedDefaults(defaults: Readonly<Record<ParamId, number>>): void {
    for (const [id, v] of Object.entries(defaults)) {
      if (Object.prototype.hasOwnProperty.call(defaults, id) && v !== undefined) {
        this.params.set(id as ParamId, v);
      }
    }
  }

  reset(): void {
    this.params.clear();
  }
}

/** The multiband panel's resolved parameter state.
 *
 * `values` is keyed by `ParamId` and holds the live value per parameter.
 * `setValue` commits a change through the source.
 */
export interface MultibandParamState {
  values: Readonly<Record<ParamId, number>>;
  setValue: (id: ParamId, value: number) => void;
}

/**
 * Resolve every multiband param from a `ParamSource` and expose a single
 * `setValue` to the panel.
 *
 * The default source is a `LocalParamSource`, so the panel renders and can be
 * tested with no engine. To run against the engine, provide an
 * `EngineParamSource` (see the engine card) that reads the rig snapshot and
 * writes the `setParam` command.
 */
export function useMultibandParams(
  source: ParamSource,
  block: Block,
): MultibandParamState {
  return useMemo(() => {
    const values = {} as Record<ParamId, number>;
    for (const schema of ALL_PARAMS) {
      values[schema.id] = source.getValue(schema.id, schema.default);
    }
    return {
      values,
      setValue: (id: ParamId, value: number) => {
        source.setValue(id, value);
      },
    };
  }, [source, block]);
}

// ---------------------------------------------------------------------------
// Meter values (per-band gain reduction + output level)
// ---------------------------------------------------------------------------

/**
 * Per-band metering shape sent by the engine (extended `Meters`, see
 * docs/design/multiband-compressor.md §7) and the stub used when no engine is
 * present. `gr_db` is the peak gain reduction of the band (dB, negative = red
 * reduction), `out_db` the peak output level (dBFS).
 */
export interface MultibandMeter {
  band: string;
  gr_db: number;
  out_db: number;
}

export interface MultibandMeters {
  in: number;
  out: number;
  cpu: number;
  xruns: number;
  bands: ReadonlyArray<MultibandMeter>;
}

/** Local, engine-less source of per-band meter values.
 *
 * Starts flat (no gain reduction yet) and is independent of the audio thread, so
 * the panel can render placeholder GR meters in the skeleton.
 */
export class LocalMetersSource implements MultibandMeters {
  in = 0.42;
  out = 0.38;
  cpu = 0;
  xruns = 0;
  bands: readonly MultibandMeter[] = [];

  /** Reset to the fresh-rig state. */
  reset(): void {
    this.in = 0.42;
    this.out = 0.38;
    this.cpu = 0;
    this.xruns = 0;
    this.bands = [];
  }
}

/** Resolve the per-band GR meters the panel renders. */
export function useMultibandMeters(meters: MultibandMeters): MultibandMeters {
  return meters;
}
