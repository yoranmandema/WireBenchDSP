# Block / channel view — verbosity audit

**Scope:** the block/channel view only — `ui/src/components/Chain.tsx` and the
chain/block/channel classes in `ui/src/styles.css` (plus, for context, the block
editor header and the rack, which share the same visual language).

**Method:** rebuilt `ui/dist` from the current source (`npm run build`, clean
`tsc`), opened the real bundle in headless Chrome against the running engine,
and catalogued every text node, badge, icon, tooltip and state colour from the
live DOM + computed styles. Live state: 4 channels, Channel 1 holding 5 blocks
(Amp, Overdrive, Cab, Pitch Delay, Reverb), Channels 2–4 empty.

**Result:** the chain renders **26 text-bearing elements** and 10 small icon
buttons for 5 blocks. Per block card, the name text alone occupies 38 px of the
66 px card height (58 %).

---

## 1. Inventory — per channel (each row rendered 4×)

| # | Element | Renders | Content (live value) | Information | Essential? | Could become |
|---|---------|---------|----------------------|-------------|------------|--------------|
| C1 | `.chain-label` | 1 | `Signal chain` (10 px, letter-spaced) | panel identity | **Decorative** — the panel is self-evident | delete, or a 12 px glyph |
| C2 | `.channel-name` | 4 | `Channel 1` … `Channel 4` (11 px) | which channel | **Essential** (identity) | a number chip `1`–`4`; the word "Channel" is redundant ×4 |
| C3 | `.channel-head input[type=range]` | 4 | no visible text; tooltip `Volume 0.0 dB` | channel volume | **Essential** | keep; tooltip is the only label — inconsistent with the rest |
| C4 | `label.mini` (checkbox) | 4 | `In` (10 px) | input source enabled | **Essential** but **duplicates C5** | jack / arrow-in icon + on/off colour |
| C5 | `.io.in` | 4 | `In` (10 px, boxed) | signal enters here | **Decorative** — same word as C4, and the connector line already shows flow | a port dot / arrow shape, no text |
| C6 | `.io.out` | 4 | `Out` (10 px, boxed) | signal leaves here | Semi-essential | arrow / port shape, no text |
| C7 | `.strip-empty` | 3 (empty channels) | `Empty channel — press + to search for a block` (11 px) | how to fill the channel | **Decorative** / instructional, repeated 3× | `Empty` + the existing `+`, or a dashed drop zone |
| C8 | `.add-block` | 4 | `+` icon (16 px in a 34 px button), title `Search and add a block` | add a block | **Essential** | keep as-is (already icon-only) |
| C9 | `.hint` | 1 | `Drag to reorder · click a block to edit it below.` (10 px) | interaction help | **Decorative** | delete, or a first-run-only hint |

**Channel header today:** `Channel 1  [slider]  In`  →  then the strip repeats
`In … Out`. The word **"In" appears twice in every channel row** (C4 + C5).

## 2. Inventory — per block card (each row rendered 5×)

| # | Element | Renders | Content | Information | Essential? | Could become |
|---|---------|---------|---------|-------------|------------|--------------|
| B1 | `.block-icon` svg | 1 | block/category glyph, 26 px, category hue | block type | **Essential** (visual) | keep, enlarge to 32–40 px |
| B2 | `.block-name` | 1 | `Amp`, `Overdrive`, `Cab`, `Pitch Delay`, `Reverb` (11 px, wraps to 2 lines, 38 px tall) | block identity | **Essential** but **verbose** — dominates the card | one short word, or icon + tooltip only |
| B3 | `.icon-btn.bypass` | 1 | power glyph 10 px inside 16×16 | bypass toggle | **Essential** | keep, but reveal on hover; 16 px is far below touch size |
| B4 | `.icon-btn.remove` | 1 | `×` glyph 9 px inside 16×16 | delete block | **Essential** | merge with B3 into one overflow menu, or hover-reveal |
| B5 | card `title` tooltip | 1 | `Amp (amp.basic) — drag to reorder, click to edit` | identity + gestures | Semi — the **raw type id is developer-facing**, not player-facing | `Amp — drag to move`; move the type id to the editor |
| B6 | card colour, port dots, glow | — | 3 px category border, two 5 px port dots, selection ring/glow, bypass dim + strikethrough | category, selection, bypass | **Essential** (non-text) | keep — lean on it more so text can go |
| B7 | `.drag-ghost` | on drag | icon (14 px) + block name | dragged block | **Essential** | keep |

**Per card today:** a 26 px icon above a 38 px two-line name, flanked by two
always-visible 16 px buttons. 10 tiny buttons across 5 cards is the main source
of visual noise.

## 3. Adjacent surfaces (context — not the chain, but same problem)

| # | Element | Content | Essential? | Note |
|---|---------|---------|------------|------|
| E1 | `.editor-kind` | `Selected block` (9 px) + category icon | **Decorative** | the name + colour already say it |
| E2 | `.editor-name` | block name (16 px) | Essential | duplicates the card's name (B2) |
| E3 | `.editor-type` | `time.pitchdelay` (9 px mono) | **Decorative** | raw registry id; same one already in the card tooltip (B5) |
| E4 | `.side-btn` ×2 | `Bypass`, `Reset` (11 px) | Essential | fine |
| R1 | `.rack h2` ×6 | category name + icon (10 px) | Essential | fine |
| R2 | `.rack-item` ×11 | icon + name (13 px) | Essential | fine |
| R3 | rack item `title` | `Add Amp (amp.basic) — or drag it into a channel` | Semi | verbose; keep name + "drag", drop the id |

---

## 4. Ranked verbosity hotspots (top 12)

Ranked by (repetition × characters × how safely it can be replaced). "Labels
removed" counts text elements, not characters.

| Rank | Element | Why it is a hotspot | Action | Labels removed |
|------|-----------------------------|--------|----------------|
| 1 | `.io.in` **and** `label.mini` "In" | The same word twice in every channel row (8 labels total); the header checkbox and the strip endpoint say nothing different | Replace both with one jack/arrow-in glyph + an on/off state colour; keep at most one endpoint marker | **8** |
| 2 | `.strip-empty` | 34-char instruction repeated on every empty channel | `Empty` + the existing `+`; or a dashed drop zone with no text | 3 (shortened) |
| 3 | `.block-name` | 11 px text wrapping to 38 px = 58 % of the card; the card is text-first, not icon-first | Enlarge the icon to 32–40 px; keep a single short word (or move the name to the tooltip) | 0–5 |
| 4 | `.icon-btn.bypass` + `.icon-btn.remove` | Two always-visible 16×16 buttons per card (10 total) — noise, and below any touch target | Reveal on hover/focus, or one `⋯` menu; if kept visible, ≥24 px | 0 (text-free) |
| 5 | `.chain-label` + `.hint` | Two chrome sentences describing the obvious panel and its drag/click behaviour | Delete both; keep a first-run hint only | **2** |
| 6 | card `title` tooltip | Carries the raw registry id (`amp.basic`) and gesture prose to every player | `Name — drag to move`; move the type id to the editor detail | 0 (tooltip) |
| 7 | `.channel-name` | The literal word "Channel" ×4 adds no information over a number | Number chip `1`–`4` (+ optional colour dot) | 0 (shortened ×4) |
| 8 | `.io.out` | Text endpoint marker; the connector line and port dots already show direction | Arrow / port shape, no text | **4** |
| 9 | `.editor-kind` "Selected block" | Restates what the 16 px name and the category colour already show | Delete the label, keep the icon + name | **1** |
| 10 | `.editor-type` | Raw registry id shown twice (here and in the card tooltip) | Remove from the header; keep it in a detail/tooltip if needed | **1** |
| 11 | rack item `title` | `Add X (type) — or drag it into a channel` on every row | `X — drag to a channel` (drop the type id) | 0 (tooltip) |
| 12 | `.channel-head` slider | No visible label at all — the only affordance is a tooltip, inconsistent with a header that is otherwise all text | Add a tiny `VOL` glyph / speaker icon, or accept the tooltip | 0 |

## 5. Label budget

| | text elements in the chain |
|---|---|
| today | **26** |
| after ranks 1, 2, 5, 7, 8 | **12** (−54 %) |

The 12 that remain: 4 channel numbers, 5 block names, 3 `Empty` placeholders.
That meets the redesign card's "~50 % fewer labels" target with room to spare,
and it removes every duplicated or developer-facing string.

## 6. Constraints the redesign must respect

- **Do not lose:** which channel is which (C2), channel volume (C3), whether the
  input source is enabled (C4), bypass state and selection (B3/B6), block
  identity (B1/B2), add (C8) and delete (B4).
- **Colour already carries category** (`--cat-h` per block, port dots, selection
  glow). The icon is the primary type signal; the name is the fallback.
- **Bypass is currently conveyed twice** (opacity + strikethrough on the name).
  If the name is removed or shortened, keep the strikethrough/opacity so bypass
  survives without text.
- **A11y:** every icon that replaces a label needs an `aria-label`/`title` with
  the removed text (the redesign's acceptance already requires this).
- **Touch:** the two 16 px card buttons are the only sub-24 px targets in the
  view; whatever replaces them should be ≥24 px hit area.
