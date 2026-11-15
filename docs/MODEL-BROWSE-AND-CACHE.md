# Model Browsing UX & Local Model Cache — Specification

Status: **v1.1 — agreed contract** (downstream: `t_c579c084` index builder, `t_31ffa4e6` browsing UI)
Scope: how a user discovers, filters, inspects and downloads NAM / IR models, and the
on-disk contract for the local index and the downloaded files.
Non-goal: any particular upstream API. This document is written so the browsing UI and
the index builder can be built independently and still agree.

---

## 1. Concepts

| Term | Meaning |
|---|---|
| **Model** | One downloadable capture artifact. Has a `format` (`nam` \| `ir`) and a `kind` (see §4). |
| **Tone** | The upstream grouping that contains one or more models (a capture set: e.g. one A2 amp model + one IR). A model belongs to exactly one tone. |
| **Index** | A local, refreshable JSON snapshot of every browsable model. Browsing reads **only** the index. |
| **Library** | The subset of models whose files are present on disk. Derived from `local_path`, never stored separately. |

Three rules that everything else follows:

1. **Browsing is offline.** Search, filter, sort and detail read the index. The network is
   touched only by *Refresh index* and *Download*.
2. **The index is the only source of truth for the UI.** The UI never fetches a model
   record on demand.
3. **Downloads are never implicit and never destructive.** A download is a user action;
   refreshing the index never deletes or overwrites a file.

---

## 2. Information architecture

Entry points:

- **TopBar → “Models”** opens the **Model Browser** overlay. On desktop it is a centered
  modal (max 1100×720); on the Pi touchscreen it is a borderless full-screen layer.
- **NAM block editor → Model row → “Browse…”** opens the same component in **picker mode**,
  where the primary action is *Use in NAM block*.
- The preset browser is unaffected.

One component, two modes:

| | Browse mode | Picker mode |
|---|---|---|
| Primary action | Download | Use in NAM block |
| Download | shown | shown as “Download then use” |
| Target | — | the NAM block that opened it |
| Close | ✕ / Esc | ✕ / Esc (no change) |

---

## 3. Search behaviour

- One free-text field, placeholder **“Search name, author, tag…”**.
- Case-insensitive, diacritic-folded, whitespace-split into tokens. A record matches when
  **every token** matches at least one searched field (AND across tokens).
- A token matches by **substring** (`mes` → “Mesa”, `oogie` → “Boogie”, `rectif` →
  “Rectifier”). There is no word-boundary logic: the text typed is the text searched.
- Searched fields, with ranking weights (used only to order results, never to exclude
  unless the token matches nothing):

  | Field | Weight |
  |---|---|
  | `metadata.name` | 10 |
  | `metadata.author.name`, `author.username` | 8 |
  | `metadata.tags[]` | 6 |
  | `metadata.manufacturer`, `metadata.gear_model` | 5 |
  | `metadata.tone_title` | 4 |
  | `metadata.description` | 2 |

- Debounce **150 ms**; runs entirely in memory over the index. No network, no per-keystroke
  re-filter of the DOM (the list is virtualized).
- Result count is always visible: **“312 of 1,204”**.
- Matched substrings are highlighted in the list (name, author, tags only — not the
  description).
- Empty query ⇒ no ranking boost; the current sort order applies.
- Keys: `/` focuses search (while the browser is open), `Esc` clears it, `Esc` again closes
  the browser.

---

## 4. Filter taxonomy

All facets are **multi-select**, **OR within a facet**, **AND across facets**. Active
facets appear as removable chips; **Clear all** resets them. Facet value lists and counts
are computed from the current result set (standard faceted search), so counts stay
truthful after other filters are applied.

| Facet | Values | Source |
|---|---|---|
| **Kind** | `amp`, `pedal`, `outboard`, `ir`, `fullrig`, `other` | normalized `metadata.kind` |
| **Manufacturer** | free-form, sorted by count | `metadata.manufacturer` |
| **Cab / IR** | `with-cab`, `without-cab`, `ir-only` | `metadata.with_cab` + `metadata.kind` |
| **Tags** | free-form, sorted by count, top 20 then “more” | `metadata.tags[]` |
| **Format** | `nam`, `ir` | `metadata.format` |
| **State** | `all`, `downloaded`, `not-downloaded`, `stale` | `local` / `local.stale` |

### 4.1 Kind normalization

The index stores a normalized `kind`; the indexer derives it from the upstream tone. The
mapping below is the contract the index builder implements. Unknown upstream values map
to `other` and remain browsable.

| Upstream `gear` | `format` | `kind` | with-cab |
|---|---|---|---|
| `amp-cab` | nam | `amp` | yes |
| `amp` | nam | `amp` | no |
| `pedal` | nam | `pedal` | no |
| `outboard` | nam | `outboard` | no |
| `ir`, `space` | ir | `ir` | — |
| `fullrig` | nam | `fullrig` | yes |
| anything else | any | `other` | no |

`manufacturer`, `gear_model`, `cab`, `mics` are extracted by the indexer (from the model
file metadata where available, otherwise parsed from the tone description) and stored as
plain strings. They are optional; the UI omits absent fields.

**Derivation rules the indexer must implement:**

- `kind`: normalize on the upstream `gear` first. If `gear` is absent or unknown, fall back
  to `format` (`ir` → `ir`, `nam` → `other`). If `gear` is known but the `format`
  contradicts it (e.g. `gear: pedal` with `format: ir`), the record is `other` — a
  contradictory pair is never presented as a real kind.
- `with_cab`: `true` for `amp` (from `amp-cab`) and `fullrig`; `false` for `pedal`,
  `outboard`, `ir` and `other`. Stored explicitly as `metadata.with_cab` so the Cab/IR
  facet is a plain boolean filter and never depends on the free-form `cab` string.
- `variant`: upstream architecture version `"2"` → `a2`, `"1"` → `a1`, anything else (or an
  `_a1` / `_a2` filename suffix, when the version is absent) → `custom`.

---

## 5. Presentation: list vs grid

A toolbar toggle (⛶ Grid / ☰ List) switches views. The choice is persisted in
`$PEDAL_HOME/prefs.json` as `models.view` (`"list" \| "grid"`). Default: **list** on
desktop, **grid** on a touchscreen (detected via `pointer: coarse`).

### 5.1 Browser frame (shared)

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Models                                                            ✕         │
├────────────────────────────────────────────────────────────────────────────┤
│ [🔍 Search name, author, tag…                          ]  312 of 1,204      │
│                                                                            │
│ [Kind ▾] [Manufacturer ▾] [Cab/IR ▾] [Tags ▾] [Format ▾] [State ▾]        │
│                                                                            │
│ filters: [amp ×] [mesa boogie ×] [high gain ×]              Clear all      │
│                                                                            │
│ ⇅ Sort: [Most downloaded ▾]                     [☰ List] [⛶ Grid]  ⟳      │
├────────────────────────────────────────────────────────────────────────────┤
│ …results…                                                                   │
└────────────────────────────────────────────────────────────────────────────┘
```

- `⟳` = Refresh index. It shows a spinner while running and never blocks the list.
- The stale banner (§8) sits between the filter row and the results.

### 5.2 List view (default desktop, keyboard-first)

Dense, one model per row, virtualized (only visible rows are in the DOM). Column alignment
is fixed so scanning is fast.

```
┌────────────────────────────────────────────────────────────────────────────┐
│ █ Mesa Boogie Dual Rectifier Rev F — Red Channel + OD808   ⬇ 46,255  ★ 669 │
│   AmalgamAudio · mesa boogie · dual rectifier · high gain                  │
│ █ Fender Super Reverb 1965 — Edge of Breakup (A2)              ✓ downloaded │
│   AmalgamAudio · fender · super reverb · clean                              │
│ █ API 512c — left mic gain 5                                  ↓  not downloaded│
│   TONE3000 · studio · preamp · outboard                                     │
└────────────────────────────────────────────────────────────────────────────┘
```

Row contents: thumbnail (40 px, optional), name, author, top 3 tags, stats, state badge
(`✓ downloaded`, `↓ not downloaded`, `↻ update`). Whole row is the click target for the
detail view; the trailing download icon is a separate button (44 px hit area on touch).

### 5.3 Grid view (default touch)

```
┌───────────────┐  ┌───────────────┐  ┌───────────────┐
│ ┌───────────┐ │  │ ┌───────────┐ │  │ ┌───────────┐ │
│ │ thumbnail │ │  │ │ thumbnail │ │  │ │ thumbnail │ │
│ └───────────┘ │  │ └───────────┘ │  │ └───────────┘ │
│ Mesa Boogie…  │  │ Fender Super… │  │ API 512c — …  │
│ AmalgamAudio  │  │ AmalgamAudio  │  │ TONE3000      │
│ #metal #nam   │  │ #fender #clean│  │ #preamp       │
│ ⬇ 46,255 ✓    │  │ ⬇ 12,104 ✓    │  │ ↓ download    │
└───────────────┘  └───────────────┘  └───────────────┘
```

Cards are a fixed 180 px wide, reflow by available width. Thumbnails are loaded lazily
from `metadata.thumbnail` and cached; a generic amp/pedal/IR glyph is used when absent.

---

## 6. Sorting

| Option | Field |
|---|---|
| Relevance | search score (only enabled when a query is present) |
| Name A→Z | `metadata.name` |
| Author A→Z | `metadata.author.name` |
| Newest | `metadata.published_at` desc |
| Most downloaded | `metadata.stats.downloads` desc |
| Most favorited | `metadata.stats.favorites` desc |
| Recently updated | `metadata.updated_at` desc |
| Recently added | `local.downloaded_at` desc (downloaded first) |

Default: **Relevance** when a query is active, otherwise **Most downloaded**. The choice
is persisted in `prefs.json` as `models.sort`. Sorts are stable (ties broken by `id`).

Wire values (the `sort` argument to `listModels`, see §10):

| Label | Wire value |
|---|---|
| Relevance | `relevance` |
| Name A→Z | `name` |
| Author A→Z | `author` |
| Newest | `newest` |
| Most downloaded | `downloads` |
| Most favorited | `favorites` |
| Recently updated | `updated` |
| Recently added | `added` |

---

## 7. Detail view

Clicking a row/card opens the detail view: a right-hand pane on desktop (520 px), a
full-screen sheet on touch. It lists every metadata field the record has; absent fields
are omitted, never shown as “null”.

```
┌───────────────────────────────────────────────────────────────┐
│ ← Back            Mesa Boogie Dual Rectifier Rev F — Red…  ✕  │
├───────────────────────────────────────────────────────────────┤
│  ┌────────┐   Mesa Boogie Dual Rectifier Rev F — Red Channel  │
│  │ thumb  │   + OD808                                          │
│  │        │   by AmalgamAudio  ✓ verified                      │
│  └────────┘   ★ 669   ⬇ 46,255   ␣ license t3k                │
├───────────────────────────────────────────────────────────────┤
│  Kind          amp (with cab)                                 │
│  Format        NAM · architecture A2                          │
│  Sample rate   48 kHz                                         │
│  Loudness      −22.98 LUFS      Gain 0.0                      │
│  Manufacturer  Mesa Boogie                                    │
│  Gear model    Dual Rectifier Rev F 1993                      │
│  Cabinet       4×12 Mesa, Celestion V30                       │
│  Mics          SM57, R121, U87                                │
│  Tags          metal · nam · high gain · mesa boogie · …      │
│  License       t3k                                            │
│  Links         amalgamcaptures.com/… · youtu.be/…             │
│  Source        tone3000.com/tones/…-106343                     │
│  Local         ✓ models/t3k/106343/379978_a2.nam              │
│                301,225 B · sha256 388d6d0e… · 2026-11-14      │
│  Updated       2026-10-30 15:39 UTC                            │
├───────────────────────────────────────────────────────────────┤
│  DESCRIPTION                                                  │
│  Get the full capture pack … (wrapped, expandable “…more”)    │
├───────────────────────────────────────────────────────────────┤
│  [        Download        ]   [     Use in NAM block     ]    │
└───────────────────────────────────────────────────────────────┘
```

The right-hand **Use in NAM block** button exists only in picker mode; in browse mode the
detail view shows **Download** alone, because there is no target block.

Download button states: `Download` → progress bar + `Cancel` → `Downloaded ✓` (with a
“Re-download” affordance). If `local.stale`, the primary button reads
`Update available — Download`.

In picker mode the right-hand button is always **Use in NAM block**; it is disabled until
the model is downloaded, with the hint “Download first”.

---

## 8. Empty / error / stale states

| State | What the user sees | What is still possible |
|---|---|---|
| **No index** | Empty state: “No model index yet” + primary **Download the model index** | nothing else |
| **Stale index** | Amber banner: “Index is N days old · Refresh” | full browsing from cache |
| **Refreshing** | Toolbar spinner + “Refreshing…”; list stays interactive | browsing |
| **Refresh failed** | Toast: “Couldn't refresh — still showing the cached index (N days old)” | browsing from cache |
| **Offline** | Same as refresh failed; banner shows “Offline — cached index” | browsing |
| **No results** | “No models match ‘…’ · Clear filters” | clear filters |
| **Download failed** | Per-row error + Retry; existing file untouched | retry |

Rules: a failed refresh never clears or truncates the index; a failed download never
removes an existing file; the UI never shows a spinner over the whole results area.

**Stale threshold.** `PEDAL_INDEX_STALE_DAYS = 7`. The banner appears when `generated_at`
is older than 7 days and disappears after a successful refresh. Engine and UI both derive
it from `generated_at`, so they agree.

---

## 9. Local storage contract

### 9.1 Paths

Everything lives under **`PEDAL_HOME`**:

| Platform | Default `PEDAL_HOME` |
|---|---|
| macOS | `~/Library/Application Support/app.pedal/` |
| Linux / Pi 5 | `~/.local/share/app.pedal/` |
| Windows | `%APPDATA%\app.pedal\` |

**Dev checkout detection.** If an ancestor of the running executable contains `presets/`,
`models/` and `Cargo.toml` (i.e. the repo), `PEDAL_HOME` defaults to that repo root, so
development uses the same layout the presets already use. `PEDAL_HOME` (env) always wins.

```
$PEDAL_HOME/
├── index/
│   ├── models.json          # the cached index (canonical, always present after first refresh)
│   ├── models.json.tmp      # in-flight write; renamed over models.json
│   └── models.json.bak      # previous good index, kept for one generation of rollback
├── models/
│   └── <provider>/<tone_id>/<model_id>_<variant>.<ext>
└── prefs.json               # view mode, sort, facet state
```

- `PEDAL_MODELS` (env) overrides the `models/` directory, mirroring `PEDAL_PRESETS`.
- `index/models.json` is the **only** index file. Multiple providers add records, not files.
- `index/` is gitignored in the repo (dev mode).

### 9.2 Download destination layout

```
$PEDAL_MODELS/<provider>/<tone_id>/<model_id>_<variant>.<ext>
```

Example: `$PEDAL_HOME/models/t3k/106343/379978_a2.nam`

- `ext` = `nam` for `format: nam`, `wav` for `format: ir`.
- `variant` ∈ `a1` \| `a2` \| `custom`, from the record.
- The file name is derived from the **model id and variant only** — never from the display
  name, so it is stable and collision-free (two tones cannot produce the same path).
- `local_path` in the index is stored **relative to `$PEDAL_MODELS`**, POSIX separators
  (`t3k/106343/379978_a2.nam`), so an index is portable between machines.

Download algorithm (per model):

1. If `local_path` exists and its sha256 matches `local.checksum` ⇒ already downloaded, done.
2. Download to `<final>.part` (no resume in v1; `.part` is deleted on failure).
3. Verify sha256. On mismatch ⇒ delete `.part`, report failure.
4. If `<final>` already exists (hash mismatch) ⇒ move it to `<final>.bak`, then rename
   `.part` → `<final>`.
5. Atomically rewrite `models.json` with the new `local` block, then delete the `.bak`.

Downloads run through a small sequential queue (max 2 concurrent), are cancellable, and
report progress (bytes / total). The index is updated **after** each success, so a crash
can never leave `local_path` pointing at a missing file.

### 9.3 Index record schema

Canonical, machine-readable schema: **`model-index.schema.json`** (draft 2020-12).
A worked example with two records: **`example-models.json`**.

Top level:

```jsonc
{
  "schema": 1,                     // integer, the schema version
  "provider": "t3k",               // primary provider id
  "generated_at": "2026-11-14T23:40:00Z",
  "count": 1204,
  "models": [ /* ModelRecord */ ]
}
```

ModelRecord — **required fields** (one authoritative set, matching the schema): `id`,
`provider`, `tone_id`, `source_url`, `fetched_at`, `metadata`, `local_path`, `local`.
`local` is always present and is `null` when the model is not downloaded; `local_path` is
`null` in that same case. Inside `metadata`, `name`, `author`, `kind`, `format` and
`variant` are required; every other metadata field is optional and omitted when unknown.

```jsonc
{
  "id": "t3k:379978",              // "<provider>:<model_id>"; stable across refreshes
  "provider": "t3k",
  "tone_id": "106343",
  "source_url": "https://www.tone3000.com/tones/…-106343",   // landing page
  "download_url": "r2://models/9sntd682muo_a2.nam",         // null if not resolvable
  "content_hash": "nam-content-v1:388d6d0e…",               // null if upstream has none
  "fetched_at": "2026-11-14T23:40:00Z",                     // RFC3339, per record
  "local_path": "t3k/106343/379978_a2.nam",                 // relative; null if not downloaded
  "local": {
    "file_name": "379978_a2.nam",
    "size_bytes": 301225,
    "checksum": "sha256:329f6884…",
    "downloaded_at": "2026-11-14T23:41:02Z",
    "stale": false
  },
  "metadata": {
    "name": "Mesa Boogie Dual Rectifier Rev F — Red Channel + OD808",
    "tone_title": "Mesa Boogie Dual Rectifier Rev F - Red Channel + OD808 - High Gain",
    "author": {
      "name": "AmalgamAudio",
      "username": "amalgamaudio",
      "url": "https://www.tone3000.com/amalgamaudio",
      "avatar_url": "https://assets.tone3000.com/….jpg",
      "verified": true
    },
    "description": "…full text…",
    "kind": "amp",
    "format": "nam",
    "variant": "a2",
    "manufacturer": "Mesa Boogie",
    "gear_model": "Dual Rectifier Rev F 1993",
    "cab": "4×12 Mesa, Celestion V30",
    "mics": "SM57, R121, U87",
    "tags": ["metal", "nam", "high gain", "mesa boogie"],
    "sample_rate": 48000,
    "loudness_db": -22.98,
    "gain_db": 0.0,
    "license": "t3k",
    "links": ["https://amalgamcaptures.com/…"],
    "thumbnail": "https://assets.tone3000.com/….jpg",
    "stats": { "favorites": 669, "downloads": 46255 },
    "published_at": "2026-10-30T15:39:11Z",
    "updated_at": "2026-10-30T15:39:14Z"
  }
}
```

Notes:

- All timestamps are **RFC 3339 UTC** (`Z`), never locale strings.
- `tags` are lowercased, trimmed, deduplicated and sorted.
- `kind`, `format`, `variant` are enums (see §4.1 and the schema).
- `metadata.author` is always present (a model always has an uploader); all other
  metadata fields are optional and omitted when unknown.
- `local` is `null` when the model is not downloaded; `local_path` is `null` in that case.
- `metadata` is a flat object (not nested per provider) so the UI and the schema stay
  provider-independent.

### 9.4 Refresh / update semantics

- **Manual only in v1.** Refresh is a user action (⟳). Automatic background refresh is out
  of scope; the stale banner is the only nudge. (A later `models.autoRefresh` pref may be
  added without a schema change.)
- **All-or-nothing.** Refresh builds the whole candidate index in memory, validates every
  record against the schema, then atomically replaces `models.json`. A failure on any page
  (after retries/backoff) aborts the refresh and leaves the old index untouched.
- **Identity and merge.** The candidate index starts from the **existing** index keyed by
  `id`; every existing record carries its `local` and `local_path` forward. Refresh then
  overwrites the upstream-derived fields (`metadata`, `source_url`, `download_url`,
  `content_hash`, `fetched_at`) in place. A record is never re-created, so a refresh can
  never orphan a download.
- **Deletions.** A record absent from the new upstream listing is dropped from the index,
  but its downloaded file is **never deleted**.
- **Staleness.** Compare the new upstream `content_hash` with the `content_hash` stored in
  the existing record for that `id` (captured before the overwrite). If they differ, set
  `local.stale = true` and keep the file. If either side is `null`, treat the record as
  **not stale** — never nudge the user on missing data. The user is offered “Update”; the
  file is re-downloaded only on demand.
- **`fetched_at`** = when that record's metadata was last fetched from upstream (per
  record). **`generated_at`** = when the index file was written. Both are always present.
- **Single-flight.** A second refresh request while one is running is ignored; the UI shows
  one spinner. Concurrent downloads do not trigger concurrent index writes (they queue
  behind a lock).
- **Rate limits.** Honour `Retry-After`; exponential backoff with jitter; abort with a
  clear error after the retry budget. Never spin on 429.

---

## 10. UI / engine contract (what the builders need)

The browsing UI talks to the engine over the existing IPC protocol (`docs/IPC-PROTOCOL.md`).
The command names below are **frozen** — the client task and the UI task implement exactly
these. Only the transport plumbing (registering them in `server.rs`) is left to the
builder.

| Command | Payload |
|---|---|
| `listModels` | `{ query, kinds[], manufacturers[], cabIr[], tags[], formats[], states[], sort, page, pageSize }` → `{ total, page, pageSize, models: ModelRecord[], facets }` |
| `getModel` | `{ id }` → `ModelRecord` |
| `refreshIndex` | `{}` → `{ generated_at, count }` (also emitted as `event: models` on completion/failure) |
| `downloadModel` | `{ id }` → `{ id, local_path, local }` |
| `cancelDownload` | `{ id }` → `{}` |

**Frozen wire values** (UI and engine must agree exactly):

| Field | Values |
|---|---|
| `sort` | `relevance` \| `name` \| `author` \| `newest` \| `downloads` \| `favorites` \| `updated` \| `added` |
| `kinds[]` | `amp` \| `pedal` \| `outboard` \| `ir` \| `fullrig` \| `other` |
| `cabIr[]` | `with-cab` \| `without-cab` \| `ir-only` |
| `formats[]` | `nam` \| `ir` |
| `states[]` | `all` \| `downloaded` \| `not-downloaded` \| `stale` |

`facets` is computed by the engine over the result set filtered by the query and **all other
facets** (standard faceted search), before paging:

```jsonc
{
  "kind":         [{ "value": "amp", "count": 812 }, …],
  "manufacturer": [{ "value": "Mesa Boogie", "count": 96 }, …],   // count desc
  "cabIr":        [{ "value": "with-cab", "count": 740 }, …],
  "tags":         [{ "value": "metal", "count": 512 }, …],       // count desc
  "format":       [{ "value": "nam", "count": 1180 }, …],
  "state":        [{ "value": "downloaded", "count": 42 }, …]
}
```

Notes:

- The engine owns the index and the download queue; the UI holds no index state of its own.
- `listModels` returns **full `ModelRecord` objects** — there is no separate summary type.
  At `pageSize` 100 a page is roughly 200 KB, which is fine over loopback.
- `listModels` is a **paged** query. The UI virtualizes; it never pulls 10k records at once.
  `pageSize` default 100, max 500. Paging applies **after** faceting, so `facets` and
  `total` always describe the whole match set.
- `getModel` reads the **index**, never upstream. It lets the detail pane and the picker
  load one record by id without a filtered `listModels` round-trip.
- Search/filter/sort run in the engine (Rust), so the UI stays thin and the same logic is
  available to a future native UI.

### 10.1 Preset integration (dependency, not this task)

Presets today store a **relative file path** (`"model": "../models/MES DRECF….nam"`). The
download layout above makes that fragile. Decision:

- **New presets reference `model` by index `id`** (e.g. `"model": "t3k:379978"`), resolved
  through `local_path` at load time.
- **Legacy path references keep working**: if `model` is not a known `id`, treat it as a
  path relative to the preset file, exactly as today.
- Migration of existing presets is a separate task (`kanban_create` follow-up), not part of
  this spec.

---

## 11. Accessibility & touch

- Full keyboard support: `/` focus search, `Esc` clear/close, `↑`/`↓` move, `Enter` open
  detail, `d` download, `u` use (picker mode). Visible focus ring.
- Touch targets ≥ 44 px; no hover-only affordances; the download button is a real button,
  not a hover reveal.
- The list is virtualized and facet counts are computed from the in-memory index, so a
  5k-record index stays under ~50 ms per keystroke. (If the catalog grows past ~20k, move
  faceting to a worker — the IPC shape does not change.)

---

## 12. Review pass — acceptance criteria

| Acceptance criterion | Where | Status |
|---|---|---|
| Written spec | this document | ✅ |
| ASCII wireframe: list view | §5.2 | ✅ |
| ASCII wireframe: search/filter | §5.1 | ✅ |
| ASCII wireframe: detail view | §7 | ✅ |
| Concrete JSON schema for the cached index | `model-index.schema.json` (+ §9.3) | ✅ |
| Agreed storage paths for index and downloaded models | §9.1, §9.2 | ✅ |
| Review pass | this section | ✅ |

### 12.1 Ambiguity check (deliberately resolved)

1. **Index location** — app-data dir, with a repo-dev fallback. *Resolved*: §9.1.
2. **One index or one per provider?** — one file, provider field per record. *Resolved*: §9.1.
3. **`local_path` relative or absolute?** — relative to `PEDAL_MODELS`, POSIX. *Resolved*: §9.2.
4. **Does refresh delete files for removed models?** — no, never. *Resolved*: §9.4.
5. **Does refresh auto-download changed models?** — no, marks `stale`. *Resolved*: §9.4.
6. **Which fields are required?** — `id`, `provider`, `tone_id`, `source_url`,
   `fetched_at`, `metadata`, `local_path`, `local`; inside `metadata`: `name`, `author`,
   `kind`, `format`, `variant`. One authoritative set, matching the schema. *Resolved*: §9.3.
7. **Who does search/filter/sort?** — the engine, paged. *Resolved*: §10.
8. **Preset references break with the new layout** — presets move to `id` references, legacy
   paths still resolve. *Resolved*: §10.1 (migration is a follow-up card).
9. **Multiple models per tone** — the layout is per-model (`<model_id>_<variant>`), so A1/A2/
   custom variants of one tone coexist. *Resolved*: §9.2.
10. **Grid vs list default** — list on fine pointer, grid on coarse pointer, persisted.
    *Resolved*: §5.

### 12.2 Open questions for the human (non-blocking)

- **Q1.** Should the index also be browsable on the Mac only, or does the Pi need the full
  catalog on-device? (Affects whether the Pi downloads the index over the network at first
  boot — the spec assumes yes, manually triggered.)
- **Q2.** Are IR downloads in scope for v1? The schema and layout support `format: ir`; the
  UI shows the facet. If IRs are out of scope, hide the `Format` facet and the `ir` kind.

### 12.3 Review log (this pass)

An independent adversarial review was run against v1.0. Fixed in v1.1:

- Schema: `local` is now required (always present, `null` when not downloaded), so the
  `local` / `local_path` consistency conditional no longer mis-fires when `local` is absent.
- `listModels` now returns a `facets` object; paging applies after faceting.
- Wire enum values for `sort`, `kinds`, `cabIr`, `formats` and `states` are frozen in §10.
- Staleness compares against the previous record's `content_hash`; null on either side
  means “not stale”.
- Refresh starts from the existing index and carries `local` / `local_path` forward.
- Search matching is stated as plain substring (the old wording contradicted its example).
- `metadata.with_cab` added, so the Cab/IR facet is a boolean rather than a string guess.
- Kind precedence, `with_cab` derivation and `variant` derivation are written down.
- Stale threshold fixed at 7 days; `getModel` clarified as an index read; the detail
  wireframe no longer implies a picker-only button in browse mode.

---

## 13. Out of scope

- Upstream API endpoints, auth, pagination internals (`t_c579c084`).
- Download-manager implementation (queue, resume, concurrency) beyond the contract in §9.2.
- Preset store migration to `id` references (§10.1).
- A second provider (the schema allows it; the UI is written provider-agnostically).
