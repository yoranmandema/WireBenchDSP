# Block / channel view — visual-first redesign (style note)

**Input:** [`UI-VERBOSITY-AUDIT.md`](UI-VERBOSITY-AUDIT.md) — 26 text elements and
10 icon buttons in the chain, and the ranked hotspot list.
**Deliverable:** this note, plus two mocks: [`ui-mock-chain.html`](ui-mock-chain.html)
(full panel, rendered to `ui-mock-chain.png`) and
[`ui-mock-chain-states.html`](ui-mock-chain-states.html) (state sheet, `ui-mock-chain-states.png`).

**Headline:** the chain goes from **26 visible text elements to 9** (−65 %), the block
glyph grows **26 → 32 px**, and every control becomes a **≥24 px** target. Nothing
essential is lost: each removed label survives as an `aria-label`/`title`.

---

## 1. Design idea

Three separate channels of meaning, so no single one has to carry text:

| Channel | Carries | Implementation |
|---|---|---|
| **Colour (hue)** | block *category* | existing `--cat-h` per category — card left border, icon, port dots, rack |
| **Glyph shape** | block *type* | existing 24-grid stroke icons, per-block overrides (`Icon.tsx`) |
| **Container shape** | *state* and *grouping* | chip = channel, ring = selected, dim+strike = bypassed, dashed = empty/drop |

Text is reserved for **identity only**: the channel number and the block name. Everything
else is an icon, a colour, a shape, or a tooltip.

Hierarchy, largest → smallest: **32 px block glyph → 24 px channel chip / controls →
16 px functional glyphs → 10 px name**. The card is icon-first; the name is the caption.

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
* `[←]` = the input-source toggle: one 24×24 icon button, **on** = amber glyph on an
  amber-tinted fill, **off** = muted glyph. Replaces the "In" checkbox *and* the "In" endpoint.
* Volume keeps its slider; the 16 px speaker glyph is the visible label, the dB value
  stays in the tooltip (`Channel 1 volume, 0.0 dB`).

### 2.2 Block card

```
BEFORE 116 × 66                              AFTER 116 × 64
┌──────────────┐                             ┌──────────────┐
│ ⏻  ▄▄      × │  16×16 buttons, always on   │              │  clean face
│    ▒▒        │                             │    ▄▄▄▄     │  32 px glyph
│    ▒▒        │                             │    ▒▒▒▒     │
│  Overdrive   │  11 px, wraps to 2 lines    │    ▒▒▒▒     │
│              │  38 px of 66 px = 58 %      │              │
└──────────────┘                             │  Overdrive   │  10 px, 1 line, ellipsis
                                             └──────────────┘
                                             ⏻ ×   reveal on hover / focus / selected
```

* Card is 116×64 (was 66), radius 8, padding 8, internal gap 4.
* Actions are 24×24 (was 16×16), grouped top-right, hidden until hover / focus-within /
  selection. The selected card always shows them, so the pattern is discoverable on touch.
* Name is one line at 10 px with ellipsis; the full name is in the card tooltip.

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
| Block glyph | 26 px | **32 px** | inside a 116×64 card; 40 px would crowd the name |
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
| Card | 116×64, radius 8, padding 8, gap 4 |
| Strip | gap 8, padding 18px 8px (unchanged — the room is needed for the selection ring) |
| Card actions | top 4, right 4, gap 4 |
| Empty zone | height 48, min-width 220, radius 8, 1 px dashed |
| Channel padding | 8px 10px 10px (unchanged) |

### 3.3 Type

| Role | Size | Weight |
|---|---|---|
| Block name | 10 px | 600, single line, ellipsis |
| Channel numeral | 12 px | 600, tabular-nums |
| Tooltip | 12 px | 400, `Name — gesture` at most |
| ~~"Signal chain" / hint / "Selected block"~~ | — | deleted |

### 3.4 Colour

No new hues. Reuse `--bg`, `--panel`, `--line`, `--line-soft`, `--text`, `--muted`,
`--amber`, `--cat-h` exactly as in `styles.css`.

| State | Treatment |
|---|---|
| default | `--line` border, muted chrome |
| hover | border `#4a4d59` |
| selected | 1 px `--cat` ring + `--cat` glow + 1 px lift |
| bypassed | opacity 0.5 + name strikethrough + amber power glyph |
| empty | 1 px dashed `#3f424d` |
| drop target | amber dashed + amber insertion line |
| source on | amber glyph on `#2a2318` with `--amber-dim` border |

**State is never colour-only** — every state has a second cue (ring/lift, strikethrough,
glyph fill, dash style), so it survives colour-blindness and the dimmed bypass look.

---

## 4. Label-retention rules

These are the rules the redesign is judged against.

1. **One label per entity, in one place.** An entity (channel, block, parameter) has
   exactly one visible text label. If another surface needs the same fact, it reuses the
   same label or a tooltip — never a second copy. *(Kills "In" ×2, the word "Channel",
   the duplicated editor name, the registry id shown twice.)*
2. **Keep a label only if it is identity or a live value.** Identity: channel number,
   block name. Live value: the dB reading while dragging a slider. Everything else moves
   to a tooltip or goes.
3. **Numbers and glyphs beat words.** A numeral chip replaces "Channel 1"; a speaker glyph
   replaces "Volume"; a jack glyph replaces "In"/"Out".
4. **Nothing is lost silently.** Every icon that replaces a label carries an
   `aria-label`/`title` with the removed text. Accessibility parity is a hard gate.
5. **Tooltips are overflow, not labels.** At most `Name — gesture`. Raw registry ids
   (`amp.basic`) are developer-facing and move to the editor detail, out of player tooltips.
6. **Identical repeated text is a smell.** If the same string appears on N>1 items it
   becomes one shared label or an icon. *(Kills the 34-character empty-channel sentence
   and "Channel" ×4.)*
7. **Type and state are colour/shape, never text.** Category hue + glyph shape = type;
   ring/lift = selected; dim + strike + amber = bypassed; dashed = empty/drop. No text
   restates these.
8. **Retained text is capped and bounded.** ≤ 12 visible text elements in the chain;
   each ≤ 1 line, 10–12 px, ellipsis on overflow.

### 4.1 Census

| | visible text elements |
|---|---|
| Before | **26** (4 "Channel N", 8 "In", 4 "Out", 3 empty-channel sentences, 5 block names, "Signal chain", hint) |
| **After** | **9** — 4 numerals + 5 block names |
| Delta | **−65 %** (target was ~50 %) |

Optional: a 10 px muted "Empty" in each empty zone would take it to 12 (−54 %). The mock
ships **without** it — the dashed zone plus the ⊕ is the affordance.

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
| B2 | `.block-name` 2-line 38 px | **10 px, 1 line, ellipsis** |
| B3/B4 | two 16×16 buttons | **two 24×24, hover-revealed** |
| B5 | card tooltip w/ registry id | **`Name — drag to reorder, click to edit`** |
| B6 | colour / ports / ring | keep, lean on it harder |
| B7 | `.drag-ghost` 14 px | **20 px** |
| E1 | `.editor-kind` "Selected block" | **delete** (icon + name already say it) |
| E3 | `.editor-type` raw id | **move** to editor detail |
| R3 | rack tooltip w/ id | **`Name — drag to a channel`** |

---

## 5. Accessibility

* Every icon-only control has `aria-label` **and** `title` (bypass, remove, source toggle,
  add, volume slider, channel chip).
* Source toggle exposes `aria-pressed`.
* Selection, bypass and on/off are each conveyed by **two** channels (colour + shape, or
  colour + strikethrough), never colour alone.
* Focus order: chip → volume → source → cards (left to right) → card actions. Card actions
  appear on `:focus-within`, so keyboard users reach them.
* 32 px glyphs are `aria-hidden` (the card tooltip carries the name), as today.

---

## 6. Interaction notes

* **Hover-reveal vs. touch.** Actions appear on `:hover`, `:focus-within`, and on the
  **selected** card. On a touchscreen, tapping a card selects it, which reveals the
  actions; a second tap on the action performs it.
* **Drag and drop** is unchanged. The ghost is 20 px glyph + name.
* **Empty channel**: dashed zone is a drop target; the ⊕ opens the picker. On drag-over the
  zone and the insertion line turn amber.
* **No first-run hint by default.** If one is wanted, it is a one-time, dismissible toast —
  not a permanent sentence in the panel.

---

## 7. Implementation map (`ui/src/components/Chain.tsx` + `styles.css`)

**Markup:** delete `.chain-label`, `.hint`, `.strip-empty`, `.io`; add `.channel-chip`,
`.channel-vol`, `.channel-src`, `.port`, `.empty-zone`, `.card-actions`. Move the two
`.icon-btn`s into `.card-actions`.

**CSS:** `.block-card` → 116×64 / radius 8 / `grid-template-rows: 32px 1fr` / gap 4;
`.block-name` → 10 px + ellipsis; `.icon-btn` → 24×24; `.add-block` → 32×32;
`.card-actions` → opacity 0, shown on hover/focus-within/selected.

**Tooltips:** card → `` `${name} — drag to reorder, click to edit` ``; rack item →
`` `${name} — drag to a channel` ``; editor header drops "Selected block" and the type id.

The mock's `<style>` block (`ui-mock-chain.html`) is written as a drop-in starting point:
same tokens, same class names, no framework.
