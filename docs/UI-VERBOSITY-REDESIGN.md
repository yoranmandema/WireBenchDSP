# Block / channel view — visual-first redesign (style note)

**Status:** the mock and the label-retention rules below are the artifact the
requester approves. Everything here is written against the **current** code
(`ui/src/tailwind.css` + utility classes in `ui/src/components/Chain.tsx`),
not the pre-Tailwind `styles.css` draft.

**Input:** [`UI-VERBOSITY-AUDIT.md`](UI-VERBOSITY-AUDIT.md) — 26 text elements
and 10 icon buttons in the chain, plus the ranked hotspot list. Its
`chain-view.png` attachment is the live "before" render.
**Mocks:** [`ui-mock-chain.html`](ui-mock-chain.html) → `ui-mock-chain.png`
(full panel, after) and [`ui-mock-chain-states.html`](ui-mock-chain-states.html)
→ `ui-mock-chain-states.png` (state sheet).

**Headline:** the chain goes from **26 visible text elements to 9** (−65 %),
the block glyph grows **26 → 32 px**, every control becomes a **≥24 px** target,
and the card geometry does **not** change (116×66), so nothing reflows. Nothing
essential is lost: each removed label survives as an `aria-label`/`title`.

---

## 1. Design idea

Three separate channels of meaning, so no single one has to carry text:

| Channel | Carries | Implementation |
|---|---|---|
| **Colour (hue)** | block *category* | existing `--cat-h` per category — card left border, glyph, port dots, rack |
| **Glyph shape** | block *type* | existing 24-grid stroke glyphs, per-block overrides (`Icon.tsx`) |
| **Container shape** | *state* and *grouping* | chip = channel, ring = selected, dim+strike = bypassed, dashed = empty/drop |

Text is reserved for **identity only**: the channel number and the block name.
Everything else is an icon, a colour, a shape, or a tooltip.

Hierarchy, largest → smallest: **32 px block glyph → 24 px channel chip and
controls → 16 px functional glyphs → 11 px name**. The card is icon-first; the
name is the caption.

---

## 2. Wireframes

### 2.1 Channel header

```
BEFORE (6 text elements)                    AFTER (1 text element)
┌────────────────────────────────┐          ┌────────────────────────────────┐
│ Channel 1  ━━━━●━━━━  ┌─┐       │          │ ┌──┐  🔊 ━━━━●━━━━  ┌─┐       │
│            │           │ In│     │          │ │1 │              └─┐←┐     │
│            │           └─┘      │          │ └──┘                └─┘     │
│ In ┌────┐  ┌────┐ ┌────┐  Out   │          │  ○  ┌────┐ ┌────┐ ┌────┐ ○  │
└────────────────────────────────┘          └────────────────────────────────┘
  "Channel 1" + "In"×2 + "Out"                "1"; all else is a tooltip
```

* `○` = 14 px jack-socket endpoint glyph (was the words "In" / "Out").
* `[←]` = the input-source toggle: one 24×24 icon button, **on** = amber glyph on
  an amber-tinted fill, **off** = muted glyph, `aria-pressed` carries the state.
  Replaces the "In" checkbox *and* the "In" endpoint.
* Volume keeps its slider; the 16 px speaker glyph is the visible label, the dB
  value stays in the tooltip (`Channel 1 volume, 0.0 dB`).

### 2.2 Block card

```
BEFORE 116 × 66                              AFTER 116 × 66 (geometry unchanged)
┌──────────────┐                             ┌──────────────┐
│ ⏻  ▄▄      × │  16×16 buttons, always on   │              │  clean face
│    ▒▒        │                             │    ▄▄▄▄     │  32 px glyph
│    ▒▒        │                             │    ▒▒▒▒     │
│  Overdrive   │  11 px, wraps to 2 lines    │    ▒▒▒▒     │
│              │  38 px of 66 px = 58 %      │              │
└──────────────┘                             │  Overdrive   │  11 px, 1 line, ellipsis
                                             └──────────────┘
                                             ⏻ ×   reveal on hover / focus / selected
```

* Card stays **116×66**, radius 7 (`rounded-lg`), padding 8, internal gap 4.
  Only the contents change: glyph 26 → 32 px, name one line.
* Actions are 24×24 (was 16×16), grouped top-right, hidden until hover /
  focus-within / selection. The selected card always shows them, so the pattern
  is discoverable on touch.
* Name is one line at 11 px with ellipsis; the full name is in the card tooltip.
  11 px is the floor — it is the app's `--text-sm`, and the name is the identity
  label. Dropping it to 10 px saves 1 px and costs legibility.

### 2.3 Full chain (after)

```
┌ chain panel — no "Signal chain" title, no hint ──────────────────────────────┐
│                                                                              │
│ ┌ channel ────────────────────────────────────────────────────────────────┐ │
│ │  ┌──┐   🔊 ━━━━●━━━━   [←]                                              │ │
│ │  │1 │                                                                   │ │
│ │  └──┘                                                                   │ │
│ │  ○ ┌──────┐ ┌──────┐ ┌──────┐ ┌────────┐ ┌──────┐         ⊕   ○         │ │
│ │   │ ▄▄▄▄ │ │ ∿∿∿  │ │  ◉   │ │  ⌁▟▙   │ │  )))  │                     │ │
│ │   │ ▒▒▒▒ │ │ ▒▒▒  │ │  █   │ │        │ │  )))  │                     │ │
│ │   │ Amp  │ │Overdr│ │ Cab  │ │ Pitch… │ │Reverb │                     │ │
│ │   └──────┘ └──────┘ └──────┘ └────────┘ └──────┘                     │ │
│ │     ⏻ ×      ⏻ ×      ⏻ ×      ⏻ ×       ⏻ ×   ← hover / selected     │ │
│ └────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│ ┌ channel ────────────────────────────────────────────────────────────────┐ │
│ │  ┌──┐   🔊 ━━━━●━━━━   [←]                                              │ │
│ │  │2 │                                                                   │ │
│ │  └──┘   ┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆   ⊕   ○      │ │
│ └────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 2.4 States

```
 default / hover            selected              bypassed             empty / drop target
 ┌──────────────┐      ╭──────────────╮      ┌──────────────┐      ┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆
 │ ⏻  ▄▄▄▄   × │      │  ▄▄▄▄       │      │  ▄▄▄▄       │      ┆             ┆
 │    ▒▒▒▒     │      │  ▒▒▒▒       │      │  ▒▒▒▒       │      ┆    ○        ┆
 │  Overdrive  │      │  Amp        │      │ Amp̶        │      ┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆┆
 └──────────────┘      ╰────────────╯      └──────────────┘      └─────────────┘
 24×24 actions        1 px cat ring +     50 % dim + strike    neutral dashed →
                     glow + 1 px lift     + amber ⏻            amber on drag-over
```

---

## 3. Spec

### 3.1 Icon sizes

| Element | Before | **After** | Note |
|---|---|---|---|
| Block glyph | 26 px | **32 px** | inside the unchanged 116×66 card; 40 px would crowd the name |
| Drag-ghost glyph | 14 px | **20 px** | matches the enlarged identity |
| Channel identity | text "Channel 1" | **24×24 chip**, 12 px numeral | radius 6, muted |
| Volume / speaker | none | **16 px** glyph | + 96 px slider |
| Input-source toggle | checkbox + "In" | **16 px** glyph in **24×24** | amber when on |
| Card actions | 16×16, 9–10 px glyph | **24×24**, 14 px glyph | hover-revealed |
| Add block | 34×34, 16 px | **32×32**, 18 px | |
| Endpoints | "In" / "Out" text | **14 px** jack glyph | decorative |
| Port dots | 5 px | 5 px | unchanged |
| **Minimum touch target** | 16 px | **24 px** | every interactive element |

### 3.2 Spacing

| Token | Value |
|---|---|
| Channel head | height 32, gap 8, margin-bottom 8 |
| Channel chip | 24×24, radius 6 |
| Card | 116×66 (unchanged), radius 7, padding 8, gap 4 |
| Strip | gap 8, padding 18px 8px (unchanged — the room is needed for the selection ring) |
| Card actions | top 4, right 4, gap 4 |
| Empty zone | flex 1, min-width 220, height 66 (matches a card, so the channel does not change height when the first block lands), radius 7, 1 px dashed |
| Channel padding | 8px 10px 10px (unchanged) |

### 3.3 Type

| Role | Size | Weight |
|---|---|---|
| Block name | 11 px | 600, single line, ellipsis |
| Channel numeral | 12 px | 600, tabular-nums |
| Tooltip | 12 px | 400, `Name — gesture` at most |
| ~~"Signal chain" / hint / "Selected block"~~ | — | deleted |

### 3.4 Colour

No new hues. Reuse the `@theme` tokens in `ui/src/tailwind.css` exactly:
`--color-bg`, `--color-panel`, `--color-line`, `--color-line-soft`,
`--color-text`, `--color-muted`, `--color-amber`, `--color-amber-dim`,
`--color-amber-ink`, `--color-amber-edge`, and the per-element `--cat-h` /
`cat-scope` mechanism. In Tailwind these are the utilities `bg-panel`,
`border-line`, `text-muted`, `text-amber`, `border-amber-dim`, `text-cat`, …

| State | Treatment |
|---|---|
| default | `border-line` border, muted chrome |
| hover | border `#4a4d59` (today's hover colour) |
| selected | 1 px `--cat` ring + `--cat` glow + 1 px lift (today's `.selected`) |
| bypassed | opacity 0.5 + name strikethrough + amber power glyph (today's `.bypass`) |
| empty | 1 px dashed `#3f424d` |
| drop target | amber dashed + amber insertion line (today's `.drop-target-active`) |
| source on | amber glyph on `#2a2318` with `border-amber-dim` |

**State is never colour-only** — every state has a second cue (ring/lift,
strikethrough, glyph fill, dash style), so it survives colour-blindness and the
dimmed bypass look.

---

## 4. Label-retention rules

These are the rules the redesign is judged against.

1. **One label per entity, in one place.** An entity (channel, block, parameter)
   has exactly one visible text label. If another surface needs the same fact, it
   reuses the same label or a tooltip — never a second copy. *(Kills "In" ×2, the
   word "Channel", the duplicated editor name, the registry id shown twice.)*
2. **Keep a label only if it is identity or a live value.** Identity: channel
   number, block name. Live value: the dB reading while dragging a slider.
   Everything else moves to a tooltip or goes.
3. **Numbers and glyphs beat words.** A numeral chip replaces "Channel 1"; a
   speaker glyph replaces "Volume"; a jack glyph replaces "In"/"Out".
4. **Nothing is lost silently.** Every icon that replaces a label carries an
   `aria-label`/`title` with the removed text. Accessibility parity is a hard gate.
5. **Tooltips are overflow, not labels.** At most `Name — gesture`. Raw registry
   ids (`amp.basic`) are developer-facing and move to the editor detail, out of
   player tooltips.
6. **Identical repeated text is a smell.** If the same string appears on N>1
   items it becomes one shared label or an icon. *(Kills the 34-character
   empty-channel sentence and "Channel" ×4.)*
7. **Type and state are colour/shape, never text.** Category hue + glyph shape =
   type; ring/lift = selected; dim + strike + amber = bypassed; dashed = empty/drop.
   No text restates these.
8. **Retained text is capped and bounded.** ≤ 12 visible text elements in the
   chain; each ≤ 1 line, 10–12 px, ellipsis on overflow.

### 4.1 Census

| | visible text elements |
|---|---|
| Before | **26** (4 "Channel N", 8 "In", 4 "Out", 3 empty-channel sentences, 5 block names, "Signal chain", hint) |
| **After** | **9** — 4 numerals + 5 block names |
| Delta | **−65 %** (target was ~50 %) |

Optional: a 10 px muted "Empty" in each empty zone would take it to 12 (−54 %).
The mock ships **without** it — the dashed zone plus the ⊕ is the affordance.

### 4.2 Treatment of every audited element

| Audit row | Element | Treatment |
|---|---|---|
| C1 | `.chain-label` "Signal chain" | **delete** |
| C2 | `.channel-name` "Channel 1" | **number chip `1`** + tooltip "Channel 1" |
| C3 | volume slider (tooltip only) | **speaker glyph** + slider; dB in tooltip |
| C4 | `label.mini` "In" checkbox | **24×24 icon toggle**, amber when on |
| C5 | `.io.in` "In" | **jack glyph**, no text |
| C6 | `.io.out` "Out" | **jack glyph**, no text |
| C7 | `.strip-empty` sentence | **dashed drop zone**, no text |
| C8 | `.add-block` ⊕ | keep, 32×32 |
| C9 | `.hint` | **delete** (optional first-run hint only) |
| B1 | `.block-icon` 26 px | **32 px** |
| B2 | `.block-name` 2-line 38 px | **11 px, 1 line, ellipsis** |
| B3/B4 | two 16×16 buttons | **two 24×24, hover-revealed** |
| B5 | card tooltip w/ registry id | **`Name — drag to reorder, click to edit`** |
| B6 | colour / ports / ring | keep, lean on it harder |
| B7 | `.drag-ghost` 14 px | **20 px** |
| E1 | `.editor-kind` "Selected block" | **delete** (icon + name already say it) |
| E3 | `.editor-type` raw id | **move** to editor detail |
| R3 | rack tooltip w/ id | **`Name — drag to a channel`** |

---

## 5. Accessibility

* Every icon-only control has `aria-label` **and** `title`: bypass, remove, source
  toggle, add, volume slider, channel chip.
* Source toggle exposes `aria-pressed`.
* Selection, bypass and on/off are each conveyed by **two** channels (colour +
  shape, or colour + strikethrough), never colour alone.
* Focus order: chip → volume → source → cards (left to right) → card actions.
  Card actions appear on `:focus-within`, so keyboard users reach them; while
  hidden they are also `pointer-events-none`, so they cannot be clicked by
  accident.
* 32 px glyphs are `aria-hidden` (the card tooltip carries the name), as today.

---

## 6. Interaction notes

* **Hover-reveal vs. touch.** Actions appear on `:hover`, `:focus-within`, and on
  the **selected** card. On a touchscreen, tapping a card selects it, which
  reveals the actions; a second tap on the action performs it.
* **Drag and drop** is unchanged. The ghost is a 20 px glyph + name; the
  insertion line and the `.block-placeholder` slot behave exactly as today.
* **Empty channel**: the dashed zone is a drop target; the ⊕ opens the picker. On
  drag-over the zone and the insertion line turn amber.
* **No first-run hint by default.** If one is wanted, it is a one-time,
  dismissible toast — not a permanent sentence in the panel.

---

## 7. Implementation map (current files)

Everything is Tailwind utilities; **`ui/src/tailwind.css` needs no change.**

**`ui/src/components/Chain.tsx`**

* `Chain()` — delete the `.chain-label` div and the `.hint` paragraph.
* `ChannelHead()`:
  * `.channel-name` text → `<span class="channel-chip" title={channel.name}
    aria-label={channel.name}>{numeral}</span>`; the numeral is
    `channel.name.replace(/\D/g, "")` (falls back to the full name if empty).
  * the `<label class="mini">… In</label>` checkbox → `<button
    class="channel-src" aria-pressed={on} aria-label={`${channel.name} input
    source`} title={…}>` with `<SourceIcon size={16} />`.
  * wrap the range in `<div class="channel-vol">` with `<SpeakerIcon size={16}
    />` before it, and add `aria-label={`${channel.name} volume`}`.
* `EmptyChannel()` / `PopulatedChannel()`:
  * `.io.in` / `.io.out` text divs → `<span class="port" aria-hidden="true">
    <PortIcon size={14} /></span>`.
  * **JS follow-through:** `boundaryX()` in `PopulatedChannel` measures
    `.io.in`. Rename that selector to `.port` (or to the strip's first child) or
    the insertion line lands in the wrong place.
  * empty branch: `.strip-empty` sentence → `<div class="empty-zone" />`.
  * `.add-block`: `size-8.5` → `size-8`, glyph 16 → 18.
  * `.block-card`: keep `w-[116px] h-[66px]`; `grid-rows-[auto_1fr_auto]` →
    `grid-rows-[32px_1fr]`; drop the fixed `h-[14px]` on `.block-icon`; glyph
    `size={26}` → `size={32}`.
  * `.block-name`: add `max-w-full overflow-hidden text-ellipsis
    whitespace-nowrap`; keep `text-[11px] font-semibold`.
  * move both `.icon-btn`s into `<div class="card-actions">`; `size-4` →
    `size-6`; glyphs 10/9 → 14/13.
  * `.card-actions` classes: `absolute top-1 right-1 flex gap-1 opacity-0
    pointer-events-none transition-opacity hover:opacity-100
    hover:pointer-events-auto focus-within:opacity-100
    focus-within:pointer-events-auto`, plus `opacity-100 pointer-events-auto`
    when the card is selected.
  * card `title` → `` `${b.name} — drag to reorder, click to edit` `` (drop
    `(b.type)`); add `aria-label={b.name}`.
  * drag-ghost glyph `size={14}` → `size={20}`.
  * `.block-placeholder` stays `h-[66px]`.

**`ui/src/components/Icon.tsx`** — add three functional glyphs on the same
24-grid, `currentColor`, round caps: `SpeakerIcon`, `SourceIcon` (the
arrow-into-a-terminal path already used by `BLOCK_GLYPHS["io.input"]`), and
`PortIcon` (filled socket dot + ring).

**`ui/src/components/BlockList.tsx`** line 150 — tooltip →
`` `${b.name} — drag to a channel` ``.

**`ui/src/components/Editor.tsx`** lines 105–113 — delete the `.editor-kind`
"Selected block" span and the `.editor-type` raw-id line.

The `<style>` block in `ui-mock-chain.html` is a drop-in starting point: same
tokens, same class names, no framework.

---

## 8. What the requester is approving

1. **The mock** (`ui-mock-chain.png`, `ui-mock-chain-states.png`): 32 px glyph,
   11 px one-line name, hover-revealed 24 px actions, number chip, jack
   endpoints, dashed empty zone.
2. **The label-retention rules** (§4), and the census they produce: 26 → 9
   visible text elements.
3. **Three open choices**, each with a recommendation:
   * **Empty channel text.** No text (9 labels) — recommended; the dashed zone +
     ⊕ is the affordance. Alternative: a 10 px "Empty" (12 labels).
   * **Card actions.** Hover-reveal, always visible on the selected card —
     recommended; keeps the card face clean and still touch-discoverable.
     Alternative: always visible (noisier, 10 extra targets).
   * **Block name.** Kept at 11 px, one line — recommended. Alternative: 10 px,
     or tooltip-only (drops the card to 4 labels but loses at-a-glance identity).
