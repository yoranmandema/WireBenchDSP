# Per-block-type visual style system for editors

**Status:** design — card `t_1f8bb3f8` (architect)
**Audience:** `pedal-ui-builder`
**Contract:** this document is the specification. A UI builder implements it
without asking the architect for anything beyond what it says; anything it
leaves open is a gap to report, not to guess.

---

## 1. Goal

Every block type renders in its editor with a visual *theme*: a per-block-type
colour, glyph and (optionally) artwork treatment, so that an editor for
`amp.basic` presents a faceplate/linework background on which the knobs,
labels and readouts read as parts of the amp — not as a generic control panel
floating over a neutral surface.

The system is additive: it extends the existing category colour language
(`--cat-h` from `ui/src/theme.ts`, the `cat-scope` Tailwind utility, and the
per-block glyph registry in `ui/src/components/Icon.tsx`). Nothing here
replaces those; it narrows and layers on top of them per block type.

---

## 2. Model: block type → theme

The registry maps the stable block type id (`BlockSpec.id`, e.g. `amp.basic`)
to a `BlockTheme`. Lookup is `themeFor(type)`: the block type's own entry,
then its `category` entry, then the built-in default. A missing entry is a
valid *unknown type*: it falls back to the default, it never throws.

```ts
// ui/src/theme.ts  — extend the existing Category→hue table.

export interface BlockTheme {
  /** Category this theme inherits from for hue/glyph/artwork shape. */
  category: Category;

  /**
   * Per-block artwork payload: optional inline SVG markup for the block's
   * faceplate/linework. undefined = no artwork background; the control grid
   * sits on the plain `cat-scope` surface.
   *
   * Format: XML-escaped, URL-encoded SVG (§3.3). Use `fill="currentColor"` /
   * `stroke="currentColor"` so one asset recolours itself per theme through
   * CSS colour inheritance.
   */
  artwork?: string;

  /** Tokens that make controls readable over the artwork (§3.2). */
  tokens?: {
    ink: string;      // readout / label text colour — default near-white
    face: string;     // control-panel surface — default the well
    scrim: string;    // translucency over artwork — default rgba(9,10,14,0.55)
    knobLine: string; // the rotary-ring colour — default var(--color-amber)
    accent: string;   // per-theme accent — default var(--color-amber)
  };
}

/**
 * Per-block overrides over the category default. New block types add their
 * own entry here. Keys are the registry ids (`amp.basic`), never the display
 * name.
 */
export const BLOCK_THEMES: Record<string, Partial<BlockTheme>> = {
  "amp.basic": {
    category: "Modelling",
    artwork: "<svg viewBox='0 0 24 24'>...amp faceplate...</svg>",
    tokens: {
      ink: "#0f0f0f", face: "rgba(20,22,28,0.86)",
      scrim: "rgba(18,20,24,0.55)", knobLine: "var(--color-amber)",
      accent: "var(--color-amber)",
    },
  },
  // ... (§7 has the full table)
};

/** The lookup: own entry → category entry → default. */
export function themeFor(type: string, category: Category): BlockTheme {
  const base: BlockTheme = {
    category,
    glyph: undefined, // resolved by BlockIcon (§4), not here
    artwork: undefined,
    tokens: {
      ink: "#f2f4f7",
      face: "var(--color-well)",
      scrim: "rgba(9,10,14,0.55)",
      knobLine: "var(--color-amber)",
      accent: "var(--color-amber)",
    },
  };
  const own = BLOCK_THEMES[type];
  if (own) Object.assign(base, own);
  return base;
}
```

Notes:
- `BlockSpec.category` stays the authoritative fallback. The per-block entry
  only *narrows* the design (artwork, stronger tokens), it never overrides
  the category hue/glyph.
- `icon.tsx` owns the glyph. The theme system does *not* own the glyph; it
  provides `artwork` (linework) and `tokens` (colour), and the component
  resolution `BLOCK_GLYPHS[type] ?? GLYPHS[category]` is unchanged.
- Adding a block type = (a) it gets a `BlockSpec.id` in the engine registry,
  and (b) optionally one `BLOCK_THEMES` entry. Both are independent; a new
  type without an entry inherits the category default.

---

## 3. Theme mechanism

Three layers, applied in order (later layers override earlier ones):

### 3.1 CSS custom properties (tokens)

The wrapper element gets `style={{ '--cat-h': ..., '--cat': ..., '--cat-soft': ...,
...theme.tokens }}`. The existing `@utility cat-scope` already derives
`--cat`/`--cat-soft` from `--cat-h`, so a theme that only changes colour/
readability tokens needs no new CSS. Per-theme colour sets still need `@theme`
below.

### 3.2 Tailwind tokens (`@theme`)

Add the theme's fixed tokens to `@theme` in `ui/src/tailwind.css`
(e.g. `--ink`, `--face`, `--scrim`, `--knob-line`, `--accent`):
`text-ink`/`bg-face`/`border-knob-line` utilities then read them. Artwork
colours **must not** go through Tailwind — see §3.3.

### 3.3 Inline SVG backgrounds (linework)

Artwork is an embedded SVG applied as the frame's `background-image` — a
data URI (XML-escaped, URL-encoded):

```ts
const svgUri = (() => {
  const xml = artwork.replace(/</g, "&lt;").replace(/>/g, "&gt;");
  return `url("data:image/svg+xml,${encodeURIComponent(xml)}")`;
})();
```

SVG `fill="currentColor"` / `stroke="currentColor"` resolves from the
ancestor's `color` at paint time, so one artwork asset recolours per theme
through normal CSS inheritance: `--ink` colours the bright parts, the canvas
colour the dark parts, `--accent` the highlights. The arch type is the only
one that ships a full faceplate; every other type has either a glyph or a
subtle contour, and the `tokens` table is what makes its controls readable
over it.

**Geometry rule:** the artwork fills the frame's content box at `100%`/`100%`
and is *never* scaled or cropped by the component. The block editor reports
the envelope of its artwork to the theme author (a `contentRect` callback),
so artwork authors can design to a known box rather than guessing.

---

## 4. Glyphs (reuse the existing per-block glyph registry)

`ui/src/components/Icon.tsx` already owns `BLOCK_GLYPHS` — a
`Record<string, JSX.Element>` covering 14 of the 17 registry block types,
keyed by `BlockSpec.id`:

| Block type | Category | Glyph |
|---|---|---|
| `io.input` / `io.output` | I/O | jack arrow into/out of a terminal |
| `util.gain` | Utility | level meter, 4 bars climbing |
| `util.master` | Utility | master knob, circle + cardinal ticks |
| `util.mix` | Utility | two streams merging into one |
| `util.sum` | Split/Merge | two streams merging into one |
| `dist.overdrive` | Distortion | soft clip, flattened wave |
| `tone.stack` | Tone | three-band EQ stack |
| `amp.basic` | Modelling | amp head |
| `cab.sim` | Modelling | speaker cabinet |
| `time.delay` | Time | three echoes climbing |
| `time.pitchdelay` | Pitch | pitch climbing a staircase |
| `time.reverb` | Time | diffusion: source + expanding arcs |
| `mod.nam` | Modelling | network: 5 nodes + links |
| `dynamics.compressor` | Dynamics | compressor knee (category default) |
| `mod.tremolo` | Modulation | modulation LFO (category default) |
| `pitch.octaver` | Pitch | rising arrow (category default) |

The `BlockEditorFrame` resolves the glyph exactly the way the chain view does
(`BlockIcon(type, category)`); the theme system does not duplicate it. Any
block type without an explicit glyph inherits its category glyph automatically.

---

## 5. Shared wrapper component API

`ui/src/components/BlockEditorFrame.tsx` — a single shared wrapper every
block editor uses. Existing block editors wrap themselves in it; a brand-new
block type's editor reuses it as-is.

```tsx
// ui/src/components/BlockEditorFrame.tsx

interface BlockEditorFrameProps {
  /** Stable registry id — the theme key. */
  type: string;
  name: string;
  category: Category;
  /** Optional per-block artwork (from BLOCK_THEMES). Omitted → plain frame.
   *  This is the fallback, not an error. */
  artwork?: string;
  /** The block's parameters (from the registry spec). */
  params: ParamSpec[];
  /** Current value per parameter. */
  values: Record<string, number>;
  /** Set a parameter. */
  onChange: (id: string, v: number) => void;
  /** Bypass state (null = inherited from the block, not set by the editor). */
  bypass?: boolean | null;
  /** Arbitrary children to render instead of the built-in controls. Used by
   *  the editor to keep its own logic and let the frame handle chrome. */
  children?: JSX.Element;
}

export function BlockEditorFrame({
  type, name, category, artwork, params, values, onChange, bypass, children,
}: BlockEditorFrameProps) {
  const theme = themeFor(type, category);
  const art = artwork?.length ? svgUri(artwork) : undefined;
  return (
    <section
      class="block-editor-frame"
      data-type={type}
      data-category={category}
      style={{
        "--cat-h": String(hue(category)),
        "--cat": `hsl(var(--cat-h) 62% 58%)`,
        "--cat-soft": `hsl(var(--cat-h) 45% 20%)`,
        ...theme.tokens,
        ...(art ? { "--frame-art": art } : {}),
      }}
    >
      {/* Artwork frame: fixed, full-bleed, never scrolls, never gets a solid
          background of its own, so the controls' scrim stays the only readable
          layer on top of the artwork. */}
      {art && <div class="frame-faces" style={{ backgroundImage: art }} />}

      <div class="frame-panel">   {/* controls scroll over the scrim */}
        <header class="frame-head">
          <BlockIcon type={type} category={category} size={22} />
          <div class="frame-name">{name}</div>
          {bypass === true && <span class="frame-bypass">bypassed</span>}
        </header>
        <div class="frame-controls">
          {children ?? (
            <>
              {params.map((p) => (
                <ParamControl
                  key={p.id}
                  p={p}
                  value={values[p.id] ?? p.default}
                  onChange={(v) => onChange(p.id, v)}
                />
              ))}
            </>
          )}
        </div>
      </div>
    </section>
  );
}
```

Rendered shape (desktop, `type="amp.basic"`), with the artwork behind a
translucent scrim:

```
<section class="block-editor-frame" style="--cat-h: 268; --ink:#0f0f0f; --face:rgba(20,22,28,.86); --scrim:rgba(18,20,24,.55); --knob-line: var(--color-amber); --accent: var(--color-amber); --frame-art: url(...data-uri...)">
  <div class="frame-faces" style="background-image: url(...)">   ← amp faceplate, fixed
  </div>
  <div class="frame-panel">                                       ← scrim + scroll
    <header class="frame-head">    glyph (amp.basic) · Amp · bypassed --------→
      <div class="frame-controls">  knobs + names + readouts, scrolling %}
    </div>
  </div>
</section>
```

Rules:
- The artwork is `position: fixed; inset: 0` inside the frame: full-bleed and
  stationary while the controls scroll over it. It is the *only* element with
  `background-image` from this system, so no control surface ever fights the
  artwork for contrast.
- `frame-panel` has `background: var(--face)` (or the theme `scrim`), so even
  on a busy artwork the readout text sits on something with a known contrast.
- The identity header shows the *block* glyph (fine detail) over the *category*
  hue (coarse signal) — matching the chain view, where 32 px glyph → colour →
  container shape carry three separate channels of meaning.
- `artwork` is optional by design: a block with no artwork still gets a clean,
  theme-coloured control panel. A missing entry in `BLOCK_THEMES` is not a
  parse error; it is the "no art" path.

---

## 6. Accessibility

- **Contrast (WCAG AA, 4.5:1).** `--ink` is always near-white or near-black —
  never a saturated hue at 60 %. Where the artwork is busy, the scrim
  (`var(--scrim)`) must lift the contrast ratio above AA. Each theme entry is
  only accepted after its editor screenshot is verified to pass against its
  own artwork.
- **Knobs are real controls.** A knob is `role="slider"` with `aria-label`,
  `aria-valuemin` / `aria-valuemax` / `aria-valuenow`, `tabIndex=0` and
  arrow-key handling (as `HeaderKnob` already does). The SVG arc is
  `aria-hidden` (decorative). Focus is never colour alone: the indicator turns
  the theme's `--accent` and the ring thickens, plus a visible `focus-visible`
  outline.
- **Prefers-reduced-motion.** Gating the only two CSS animations in this
  system (the knob's indicator sweep, the scrim crossfade):

```css
@media (prefers-reduced-motion: reduce) {
  .block-editor-frame .frame-controls,
  .block-editor-frame .knob-indicator {
    animation: none !important;
    transition: none !important;
  }
}
```

- **Colour is never the only signal.** A bypass, a category or a selection is
  always also expressed by shape/position (ring, glyph, label). No theme may
  rely on hue alone to mean anything.
- **`aria-label` on every artefact.** The frame's `data-type` is machine
  readable; the human-facing identity is always a label or an accessible
  `aria-label`, never a tooltip alone.
- The frame adds **no new focus patterns**: every control inside it keeps the
  existing keyboard behaviour of `Knob` / `HeaderKnob` / `ChoiceControl`. A
  builder implements the frame and the controls look and behave exactly as
  they do today.

---

## 7. Block-type style concepts

| Block type | Category | Concept (one line) |
|---|---|---|
| `amp.basic` | Modelling | Faceplate: cabinet + control face, knobs holed and amber, bezel holds the readout; the own glyph travels the head outline. |
| `cab.sim` | Modelling | Cabinet: grooved drawer fronts, a flush control panel; the glyph travels the cone outline. |
| `tone.stack` | Tone | Three-band EQ: the three faders *are* the artwork; controls overlay them. |
| `dist.overdrive` | Distortion | Stompbox: rubber feet, heavy enclosure, drive knob centred. |
| `dynamics.compressor` | Dynamics | Rackey-style: compact metal box, gate/threshold section, knee knob gold. |
| `dynamics.noise_gate` | Dynamics | Open-front box: threshold/attack/release on a small face. |
| `mod.chorus` | Modulation | Effects enclosure: rotating mode dial, rate/depth on a side panel. |
| `mod.nam` | Modelling | Desktop modeler: small-parameter grid over a "circuit" substrate. |
| `mod.tremolo` | Modulation | Lamps: light-bar shows pulse depth; rate/depth on a side panel. |
| `pitch.octaver` | Pitch | Pitch fork with octave steps; the step knob moves along it. |
| `time.delay` | Time | Tape echo: repeats climb as a tape strip; feedback/rate over a cassette face. |
| `time.pitchdelay` | Time | Tape delay with a pitch dial; a stepper says "treble/bass". |
| `time.reverb` | Time | Plate/hall: the diffusion arcs are the artwork; decay/time on a face. |
| `util.gain` | Utility | Clean meter panel: the level bars are the artwork, the gain knob overlays. |
| `util.master` | Utility | Master section: big master knob with cardinal ticks, panel face. |
| `util.mix` | Utility | Splitter/mixer face: two inputs into one output on a balanced face. |
| `util.sum` | Split/Merge | Adder ring: a small summing module on a grey panel. |
| `io.input` | I/O | Faceplate inlet: the artwork is the jack, a small "in" label over it. |
| `io.output` | I/O | Faceplate outlet: the artwork is the jack, small "out" label. |

Default (unknown type, or a type with no `BLOCK_THEMES` entry): fall through to
the category glyph + the plain `cat-scope` surface. No artwork, no new tokens.

---

## 8. Follow-up cards (implementation)

Listed so the builders can consume them one at a time; each is independent and
can be created from this text alone.

1. **`block-theme: blockTheme registry + theme tokens`** — `BLOCK_THEMES` + `themeFor`,
   default, plus the `@theme` additions in `tailwind.css`. No UI change.
2. **`block-theme: BlockEditorFrame wrapper`** — the shared wrapper and its
   slot; content drops to "plain" so it builds.
3. **`block-theme: amp faceplate artwork`** — the `amp.basic` artwork SVG + its
   `route` (per §3.3 geometry rule). The remaining artwork assets are decided
   per card from the consumer list above.
4. **`block-theme: artwork pass (cab/tone/dynamics/etc.)`** — the remaining
   artwork assets and the `tokens` that make their controls readable over them.
5. **`block-theme: accessibility pass`** — contrast pass, `prefers-reduced-motion`,
   focus states, verified against the editor screenshots.

The framework tokens (`--cat-h` → `--cat`/`--cat-soft`, `cat-scope`) and the
glyph registry are unchanged; these cards only extend them.
