# Icon selection — should it be a reusable capability?

**Question (requester, t_49aedde1):** the block/channel view needs "nice looking
icons that actually fit the blocks they represent (so not just a random UTF/emoji
character) — maybe this needs a skill/capability?"
**Sources:** `UI-VERBOSITY-AUDIT.md`, `UI-VERBOSITY-REDESIGN.md`,
`ui/src/components/Icon.tsx`, `graph/src/blocks.rs`, the existing `icon-generator`
skill. Assessment only.

## (a) What made icons succeed or fail here

**Succeeded**

- **Drawn, not typed** — hand-authored inline SVG, no emoji/UTF. This is the exact
  failure mode the requester named, and the first rule of the house style.
- **One family, one construction** — 24×24 `viewBox`, stroke 1.7, round caps/joins,
  `fill="none"`, `currentColor`; hue inherited from the parent, never hard-coded.
- **Two-level mapping** — a category archetype glyph, plus a per-block override only
  when the block is more specific than its category (cab ≠ amp, pitch delay ≠ clock,
  NAM = network). Archetypes inherit.
- **Meaning split across channels** — hue = category, shape = type, container = state;
  the icon is the primary type signal, the name the fallback caption.
- **Accessible replacement** — every icon-only control has `aria-label` + `title`;
  removed text survives as the accessible name.
- **Verified by looking** — the set is rendered at 14 px and 32/48 px and inspected.

**Failed / risky:** emoji or UTF glyphs; mixed families or stroke weights; glyphs that
read at only one size; decorative icons with no job; icons replacing a label with no
accessible name; interactive icons under a 24 px hit area.

## (b) One-off or recurring?

**Recurring, but bounded.** The current pass covers the 14 block types in
`graph/src/blocks.rs`; the roadmap adds blocks, and surfaces in flight need glyph
decisions too — add-block modal, browse/detail (download icon,
`MODEL-BROWSE-AND-CACHE.md`), drag-to-route and routing lines, preset browser,
transport. The decision procedure repeats even though the 14-glyph set is a one-time
pass.

## (c) Recommendation

**Don't build a new standalone skill — extend the existing `icon-generator` skill**
(`~/.hermes/skills/creative/icon-generator`). It already exists, already encodes the
house style, and already names `ui/src/components/Icon.tsx` as the pedal reference
set. Its gap is the *selection* half: it says "list the concepts, group them" but not
how to choose off-the-shelf vs custom, when an override is warranted, or what
acceptance checks apply. So: **build = one ~15-line patch to `icon-generator`**, with
the block-type→glyph mapping staying in-repo (`Icon.tsx`, `UI-VERBOSITY-REDESIGN.md`).

### Draft addition to `icon-generator`

```markdown
### Choosing icons (selection procedure)

Trigger: a new block/surface needs a glyph, or a set is being audited.
Inputs: concept list, category taxonomy, target sizes, existing set file.

1. Group the concepts; one family should share a construction device (same outer
   shape, different interior).
2. Source each concept in order:
   a. Reuse a glyph already in the set.
   b. Hand-draw in the house style when nothing off-the-shelf fits.
   c. Adopt an off-the-shelf family (Phosphor / Lucide / Material Symbols) only if
      the licence is MIT/Apache-2.0/CC0, the stroke and canvas match, and the whole
      family is adopted — never mix one family's glyphs with hand-drawn ones.
3. Category archetype + per-block override: draw the category glyph first; override
   only when the block's role is more specific than its category.
4. Never emoji/UTF; never a hard-coded colour — currentColor only.

Acceptance: every concept maps to one glyph (type -> glyph table); set renders in one
grid at 14 px and 48 px with no smudges; uniform stroke/canvas/weight, no mixed
families; every icon-only control has aria-label + title; interactive icons >=24 px
hit area; a render is looked at before done.
```

**Where it lives:** `creative/icon-generator/SKILL.md`. A project-scoped `pedal-ui`
skill would fragment the icon rules and duplicate the house style — not recommended.
Patch ready to apply on approval.
