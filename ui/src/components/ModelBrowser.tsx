// Model browser: search, faceted filter, sort, paginate and download NAM/IR
// models straight from the local index (read-only; the engine answers over IPC).
//
// Two modes:
//   browse — primary action is Download (the browser overlay).
//   picker — opened from a channel; primary action is "Use in NAM block".
//
// Reads only; never touches the network. The engine answers every command.

import { useEffect, useRef, useState } from "preact/hooks";
import {
  addBlock,
  closeModels,
  downloadModel,
  modelsBusy,
  modelsDetail,
  modelsDetailId,
  modelsFacets,
  modelsIndexMeta,
  modelsMode,
  modelsOpen,
  modelsPage,
  modelsPageSize,
  modelsPickerChannel,
  modelsQuery,
  modelsRefreshError,
  modelsRefreshing,
  modelsResults,
  modelsStale,
  modelsSort,
  modelsSelectedFacets,
  modelsTotal,
  modelsView,
  modelLocal,
  openModelsPicker,
  refreshIndex,
  requestModels,
  requestModel,
} from "../state";
import {
  authorName,
  authorText,
  formatBytes,
  isDownloaded,
  isStale,
  topTags,
} from "../model";
import { emptyNote, modalBackdrop, modalHead, modalShell, modalTitle, navBtn, textField } from "../ui";
import type { ModelRecord, Sort, ModelState, LocalState } from "../model";

// --- Small shared pieces ---------------------------------------------------

/** Escape arbitrary text for safe insertion into HTML (markup is added later). */
function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

/** Wrap each matched token in <mark> so the user sees why a row matched. */
function highlight(text: string, query: string): string {
  const tokens = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (!tokens.length) return escapeHtml(text);
  let html = escapeHtml(text);
  for (const token of tokens) {
    const re = new RegExp(`(${escapeHtml(token)})`, "gi");
    html = html.replace(re, "<mark>$1</mark>");
  }
  return html;
}

function kindGlyph(kind: string): string {
  const cls: Record<string, string> = {
    amp: "bg-amber",
    pedal: "bg-green",
    outboard: "bg-blue",
    ir: "bg-indigo",
    fullrig: "bg-violet",
    other: "bg-slate",
  };
  return cls[kind] ?? cls.other;
}

/** Thumbnail, lazy-loaded; a per-kind glyph is used when absent. */
function Thumbnail({ record, size }: { record: ModelRecord; size: "sm" | "lg" }) {
  const src = record.metadata.thumbnail;
  const cls = size === "sm" ? "w-10 h-10" : "w-40 h-40";
  return (
    <div class="flex shrink-0 justify-center">
      {src ? (
        <img
          class={`${cls} rounded-lg bg-inset object-cover`}
          src={src}
          alt={record.metadata.name}
          loading="lazy"
        />
      ) : (
        <div
          class={`${cls} rounded-lg ${kindGlyph(record.metadata.kind)} flex items-center justify-center text-white text-xs font-semibold`}
          title="No thumbnail"
        >
          {record.metadata.kind.toUpperCase().slice(0, 2)}
        </div>
      )}
    </div>
  );
}

// --- Detail pane ---------------------------------------------------------

/** The right-hand (or sheet) view of a single model: every metadata field. */
function ModelDetail({
  record,
  mode,
  onDownload,
  onUse,
}: {
  record: ModelRecord;
  mode: "browse" | "picker";
  onDownload: () => void;
  onUse: () => void;
}) {
  const local = record.local;
  const downloaded = isDownloaded(record);
  const stale = isStale(record);
  const [descOpen, setDescOpen] = useState(false);
  const description = record.metadata.description ?? "";

  return (
    <div class="detail flex flex-col gap-3 p-4 overflow-y-auto">
      <div class="flex items-start gap-3">
        <Thumbnail record={record} size="lg" />
        <div class="flex-1 min-w-0">
          <h2 class="text-sm font-semibold text-text leading-snug">{record.metadata.name}</h2>
          <p class="text-xs text-muted mt-0.5">
            by {authorName(record)} · {record.metadata.kind} {record.metadata.format} · {record.metadata.variant}
          </p>
          <div class="flex flex-wrap gap-2 mt-2">
            {record.metadata.stats && (
              <span class="text-[10px] text-muted">
                ⬇ {record.metadata.stats.downloads.toLocaleString()} ⭐ {record.metadata.stats.favorites.toLocaleString()}
              </span>
            )}
            {downloaded ? (
              <span class={`text-[10px] font-semibold ${stale ? "text-amber" : "text-ok"}`}>
                ✓ {stale ? "update available" : "downloaded"}
              </span>
            ) : (
              <span class="text-[10px] text-muted">not downloaded</span>
            )}
            {stale && <span class="text-[10px] text-amber">↻ update</span>}
          </div>
        </div>
        {mode === "picker" && (
          <button
            class="mini px-3 py-2 rounded-md border text-sm font-semibold bg-[#1a1b21] text-muted hover:text-text border-line"
            onClick={onUse}
            disabled={!downloaded}
            title="Use in the NAM block"
          >
            {downloaded ? "Use in NAM block" : "Download first"}
          </button>
        )}
      </div>

      <dl class="grid grid-cols-2 gap-2 text-sm">
        {record.metadata.manufacturer && (
          <div>
            <dt class="text-[10px] uppercase tracking-[0.6px] text-muted">Manufacturer</dt>
            <dd class="text-text">{record.metadata.manufacturer}</dd>
          </div>
        )}
        {record.metadata.gear_model && (
          <div>
            <dt class="text-[10px] uppercase tracking-[0.6px] text-muted">Gear model</dt>
            <dd class="text-text">{record.metadata.gear_model}</dd>
          </div>
        )}
        {record.metadata.cab && (
          <div>
            <dt class="text-[10px] uppercase tracking-[0.6px] text-muted">Cabinet</dt>
            <dd class="text-text">{record.metadata.cab}</dd>
          </div>
        )}
        {record.metadata.mics && (
          <div>
            <dt class="text-[10px] uppercase tracking-[0.6px] text-muted">Mics</dt>
            <dd class="text-text">{record.metadata.mics}</dd>
          </div>
        )}
        {record.metadata.license && (
          <div>
            <dt class="text-[10px] uppercase tracking-[0.6px] text-muted">License</dt>
            <dd class="text-text">{record.metadata.license}</dd>
          </div>
        )}
        {record.metadata.sample_rate && (
          <div>
            <dt class="text-[10px] uppercase tracking-[0.6px] text-muted">Sample rate</dt>
            <dd class="text-text">{record.metadata.sample_rate} kHz</dd>
          </div>
        )}
        {record.metadata.loudness_db != null && (
          <div>
            <dt class="text-[10px] uppercase tracking-[0.6px] text-muted">Loudness</dt>
            <dd class="text-text">{record.metadata.loudness_db.toFixed(2)} LUFS</dd>
          </div>
        )}
        {record.metadata.gain_db != null && (
          <div>
            <dt class="text-[10px] uppercase tracking-[0.6px] text-muted">Gain</dt>
            <dd class="text-text">{record.metadata.gain_db.toFixed(1)} dB</dd>
          </div>
        )}
      </dl>

      {record.metadata.tags.length > 0 && (
        <div class="text-sm">
          <dt class="text-[10px] uppercase tracking-[0.6px] text-muted mb-1">Tags</dt>
          <div class="flex flex-wrap gap-1.5">
            {record.metadata.tags.map((t) => (
              <span key={t} class="text-[10px] px-2 py-1 rounded-full bg-inset text-muted border border-line">
                #{t}
              </span>
            ))}
          </div>
        </div>
      )}

      {record.metadata.links.length > 0 && (
        <div class="text-sm">
          <dt class="text-[10px] uppercase tracking-[0.6px] text-muted mb-1">Links</dt>
          <div class="flex flex-wrap gap-2">
            {record.metadata.links.map((l) => (
              <a key={l} href={l} target="_blank" rel="noreferrer" class="text-xs text-amber hover:underline">
                {l}
              </a>
            ))}
          </div>
        </div>
      )}

      {downloaded && local && (
        <div class="text-sm bg-inset rounded-md p-3 border border-line">
          <dt class="text-[10px] uppercase tracking-[0.6px] text-muted mb-1">Local file</dt>
          <p class="text-xs text-text font-mono truncate">{local.file_name}</p>
          <p class="text-[10px] text-muted font-mono">
            {formatBytes(local.size_bytes)} · {local.checksum} · {new Date(local.downloaded_at).toLocaleString()}
          </p>
          {stale && <p class="text-[10px] text-amber mt-1">Update available — re-download when ready.</p>}
        </div>
      )}

      {description && (
        <div class="text-sm">
          <dt class="text-[10px] uppercase tracking-[0.6px] text-muted mb-1">Description</dt>
          <p
            class="text-text leading-relaxed"
            dangerouslySetInnerHTML={{ __html: descOpen ? description : description.slice(0, 220) + (description.length > 220 ? "…" : "") }}
          />
          {description.length > 220 && (
            <button class="text-xs text-amber mt-1" onClick={() => setDescOpen(true)}>
              Show more
            </button>
          )}
        </div>
      )}

      <div class="flex gap-2">
        <button
          class={`flex-1 min-h-[var(--ui-touch)] px-3.5 py-2 rounded-md text-sm font-semibold border text-white ${downloaded ? "bg-linear-to-b from-amber-hi to-amber-lo border-amber-edge" : "bg-[#1a1b21] text-muted border-line"}`}
          onClick={onDownload}
          disabled={!downloaded}
        >
          {!downloaded ? "Download" : stale ? "Update available — Download" : "Downloaded ✓"}
        </button>
        {mode === "picker" && (
          <button
            class="mini px-3 py-2 rounded-md border text-xs font-semibold bg-[#1a1b21] text-muted border-line hover:text-text"
            onClick={onUse}
            disabled={!downloaded}
          >
            Use in NAM block
          </button>
        )}
      </div>
    </div>
  );
}

// --- Main component ------------------------------------------------------

export function ModelBrowser() {
  const listScroller = useRef<HTMLDivElement>(null);
  const gridScroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [resultsHeight, setResultsHeight] = useState(0);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [searchFocused, setSearchFocused] = useState(false);
  const [focusId, setFocusId] = useState<string | null>(null);

  const results = modelsResults.value;
  const total = modelsTotal.value;
  const selected = modelsSelectedFacets.value;
  const mode = modelsMode.value;
  const query = modelsQuery.value;
  const pickerChannel = modelsPickerChannel.value;
  const detailId = modelsDetailId.value;
  const detail = modelsDetail.value;
  const indexMeta = modelsIndexMeta.value;
  const refreshing = modelsRefreshing.value;
  const busy = modelsBusy.value;
  const view = modelsView.value;
  const page = modelsPage.value;
  const pageSize = modelsPageSize.value;

  // Measure the results area so virtualization has a real height.
  useEffect(() => {
    const el = listScroller.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      for (const e of entries) setResultsHeight(e.contentRect.height);
    });
    ro.observe(el);
    setResultsHeight(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  // --- Pagination for grid view: load the next page when scrolled near the bottom.
  useEffect(() => {
    const el = gridScroller.current;
    if (!el) return;
    const onScroll = () => {
      if (el.scrollTop + el.clientHeight >= el.scrollHeight - 240 && results.length > 0) {
        if (total > page * pageSize && !busy) void requestModels();
      }
    };
    el.addEventListener("scroll", onScroll);
    return () => el.removeEventListener("scroll", onScroll);
  }, [results.length, total, page, pageSize, busy]);

  // --- Virtualized list (list mode): only visible rows in the DOM.
  const rowHeight = 64;
  const totalListHeight = results.length * rowHeight;
  const listStart = Math.max(0, Math.floor(scrollTop / rowHeight));
  const listEnd = Math.min(
    results.length,
    Math.ceil((scrollTop + resultsHeight) / rowHeight)
  );
  const visible = results.slice(listStart, listEnd);

  // --- Keyboard navigation (applies while the browser is open).
  // Re-added whenever `focusId` or `searchFocused` changes so the closure always
  // sees the latest values; signal reads (`modelsOpen`, `detailId`, …) are live.
  useEffect(() => {
    if (!modelsOpen.value) return;
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA") return;
      const k = e.key;
      if (k === "/" && !searchFocused) {
        e.preventDefault();
        const input = listScroller.current?.querySelector("input") as HTMLElement | null;
        input?.focus();
      } else if (k === "Escape") {
        if (searchFocused) {
          modelsQuery.value = "";
        } else if (detailId) {
          setFocusId(null);
          modelsDetailId.value = null;
        } else {
          closeModels();
        }
      } else if (k === "ArrowDown" || k === "ArrowUp") {
        e.preventDefault();
        if (!detailId) {
          const idx = focusId ? results.findIndex((r) => r.id === focusId) : -1;
          const next = k === "ArrowDown" ? idx + 1 : idx - 1;
          if (next >= 0 && next < results.length) {
            setFocusId(results[next].id);
            const el = listScroller.current;
            if (el) el.scrollTop = next * rowHeight;
          }
        }
      } else if (k === "Enter" && !detailId && focusId) {
        e.preventDefault();
        setFocusId(null);
        modelsDetailId.value = focusId;
        void requestModel(focusId);
      } else if (k === "d" && detailId && !refreshing && !busy) {
        e.preventDefault();
        void downloadModel(detailId);
      } else if (k === "u" && detailId && mode === "picker") {
        e.preventDefault();
        void useModel(detailId);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchFocused, focusId]);

  const useModel = async (id: string) => {
    const channel = pickerChannel;
    if (!channel) return;
    const rec = detail ?? results.find((r) => r.id === id);
    if (!rec) return;
    await addBlock(channel, rec.metadata.kind, rec.metadata.name);
  };

  const localState = (id: string): LocalState | null => modelLocal.value[id] ?? null;

  return (
    <>
      {modelsOpen.value && (
        <div
          class={`${modalBackdrop} fixed inset-0 z-50 grid place-items-center bg-[rgba(6,7,10,0.7)]`}
          onClick={() => {
            if (searchFocused) modelsQuery.value = "";
            else if (detailId) setFocusId(null);
            else closeModels();
          }}
        >
          <div
            class={`${modalShell} modalShell modal ${filtersOpen ? "w-[90vw] max-w-[1100px]" : "w-[460px] max-h-[calc(100vh-96px)]"} max-w-[1100px] flex-col overflow-hidden rounded-xl border border-[#343743] bg-linear-to-b from-[#1e2027] to-well shadow-modal`}
            onClick={(e) => e.stopPropagation()}
            role="dialog"
            aria-label="Model browser"
          >
            <header class={`${modalHead} modalHead modal-head flex-shrink-0`}>
              <h2 class={`${modalTitle} modalTitle modal-title flex-shrink-0`}>
                {mode === "picker" ? "Choose a model" : "Models"}
              </h2>
              <button
                class={`${navBtn} navBtn`}
                onClick={() => {
                  if (searchFocused) modelsQuery.value = "";
                  else if (detailId) setFocusId(null);
                  else closeModels();
                }}
                title="Close"
                aria-label="Close"
              >
                ×
              </button>
            </header>

            <div class="search flex items-center gap-2 border-b border-line-soft px-3.5 py-2.5 flex-shrink-0">
              <span class="text-[10px] tracking-[0.6px] text-muted uppercase shrink-0">Search</span>
              <input
                ref={(el) => {
                  if (modelsOpen.value) el?.focus();
                }}
                class={`${textField} textField text-md flex-1 min-w-0 outline-none`}
                value={query}
                placeholder="Search name, author, tag…"
                onInput={(e) => (modelsQuery.value = (e.target as HTMLInputElement).value)}
                onFocus={() => setSearchFocused(true)}
                onBlur={() => setSearchFocused(false)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") modelsQuery.value = "";
                }}
                aria-label="Search models"
              />
              {query && (
                <button class="text-muted hover:text-text" onClick={() => (modelsQuery.value = "")} aria-label="Clear search">
                  ✕
                </button>
              )}
            </div>

            <div class="count px-3.5 py-1.5 text-[10px] text-muted flex-shrink-0">
              {refreshing ? "Refreshing…" : `${total} of ${total > 0 ? total : "loading…"}`}
            </div>

            {modelsStale.value && indexMeta && !refreshing && (
              <div class="stale-banner flex items-center gap-2 px-3.5 py-2 bg-amber-bg border border-amber-line text-amber-text text-xs flex-shrink-0">
                <span class="text-amber">↻</span>
                <span>Index is {Math.floor((Date.now() - new Date(indexMeta.generated_at).getTime()) / 86400000)} days old.</span>
                <button class="text-amber-text font-semibold hover:underline" onClick={() => void refreshIndex()}>Refresh</button>
              </div>
            )}

            <div class="toolbar flex flex-wrap items-center gap-2 px-3.5 py-2 border-b border-line-soft flex-shrink-0">
              <button
                class={`px-2.5 py-1.5 rounded-md text-[10px] font-semibold border ${filtersOpen ? "bg-[#2f323b] text-text border-line" : "bg-[#1a1b21] text-muted border-line"}`}
                onClick={() => setFiltersOpen((v) => !v)}
                aria-expanded={filtersOpen}
                aria-controls="filters-panel"
              >
                Filters
              </button>

              <label class="sort flex items-center gap-1.5 text-[10px] text-muted">
                <span class="text-[10px] uppercase tracking-[0.6px]">Sort</span>
                <select
                  class={`${textField} textField text-xs min-w-[110px] outline-none`}
                  value={modelsSort.value}
                  onChange={(e) => {
                    const el = e.target as HTMLSelectElement | null;
                    if (el) modelsSort.value = el.value as Sort;
                  }}
                >
                  <option value="downloads">Most downloaded</option>
                  <option value="name">Name A→Z</option>
                  <option value="author">Author A→Z</option>
                  <option value="newest">Newest</option>
                  <option value="favorites">Most favorited</option>
                  <option value="updated">Recently updated</option>
                  <option value="added">Recently added</option>
                </select>
              </label>

              <div class="view toggle grid grid-cols-2 gap-1" role="group" aria-label="View">
                <button
                  class={`text-[10px] font-semibold py-1.5 px-2 border ${view === "list" ? "bg-[#2f323b] text-text border-line" : "bg-[#1a1b21] text-muted border-line"}`}
                  onClick={() => (modelsView.value = "list")}
                >
                  ☰ List
                </button>
                <button
                  class={`text-[10px] font-semibold py-1.5 px-2 border ${view === "grid" ? "bg-[#2f323b] text-text border-line" : "bg-[#1a1b21] text-muted border-line"}`}
                  onClick={() => (modelsView.value = "grid")}
                >
                  ⛶ Grid
                </button>
              </div>

              <button
                class={`refresh ${refreshing ? "text-amber" : "text-muted hover:text-text"} text-[10px] font-semibold`}
                onClick={() => void refreshIndex()}
                title="Refresh the local model index"
                aria-label="Refresh index"
              >
                {refreshing ? <span class="inline-block animate-spin">⟳</span> : <span class="inline-block">⟳</span>}
                {refreshing ? " refreshing…" : "Refresh"}
              </button>
            </div>

            {selected.kinds.length || selected.manufacturers.length || selected.cabIr.length || selected.tags.length || selected.formats.length || (selected.states.length && selected.states[0] !== "all") ? (
              <div class="chips flex flex-wrap gap-1.5 px-3.5 py-2 border-b border-line-soft flex-shrink-0">
                {selected.kinds.map((k) => <Chip key={k} label={k} onOff={() => toggleFacet("kinds", k)} />)}
                {selected.manufacturers.map((m) => <Chip key={m} label={m} onOff={() => toggleFacet("manufacturers", m)} />)}
                {selected.cabIr.map((c) => <Chip key={c} label={c} onOff={() => toggleFacet("cabIr", c)} />)}
                {selected.tags.map((t) => <Chip key={t} label={t} onOff={() => toggleFacet("tags", t)} />)}
                {selected.formats.map((f) => <Chip key={f} label={f} onOff={() => toggleFacet("formats", f)} />)}
                {selected.states[0] !== "all" && <Chip key="state" label={selected.states[0]} onOff={() => toggleFacetState()} />}
                <button class="text-muted hover:text-text text-xs" onClick={() => clearFilters()}>Clear all</button>
              </div>
            ) : null}

            {filtersOpen && (
              <div id="filters-panel" class="filters flex-1 overflow-y-auto border-b border-line-soft px-3.5 py-3 flex-shrink-0">
                {facetGroups().map((g) => (
                  <div key={g.label}>
                    <div class="flex items-center justify-between mb-1">
                      <span class="text-[10px] uppercase tracking-[0.6px] text-muted">{g.label}</span>
                    </div>
                    <div class="flex flex-wrap gap-1.5">
                      {g.values.map((v) => {
                        const facet = g.field;
                        let on: boolean;
                        if (facet === "state") {
                          on = selected.states[0] === v.value;
                        } else {
                          on = (selected[facet] as string[]).includes(v.value);
                        }
                        return (
                          <label key={v.value} class={`flex items-center gap-1 text-[10px] py-1 px-2 rounded ${on ? "text-amber" : "text-muted hover:text-text"}`}>
                            <input
                              type="checkbox"
                              checked={on}
                              onChange={() => toggleFacet(g.field as FacetField, v.value)}
                            />
                            <span class="truncate">{v.value}</span>
                            <span class="text-[9px] text-muted">{v.count}</span>
                          </label>
                        );
                      })}
                    </div>
                  </div>
                ))}
                <button class="text-xs text-amber mt-2" onClick={() => { setFiltersOpen(false); clearFilters(); }}>Clear all filters</button>
              </div>
            )}

            <div
              class="results relative flex-1 overflow-hidden min-h-0"
              onScroll={(e) => setScrollTop((e.target as HTMLElement).scrollTop)}
            >
              {!results.length && !busy && !refreshing && !modelsStale.value && (
                <div class="empty absolute inset-0 grid place-items-center">
                  <div class="text-center">
                    <div class="text-2xl mb-2">📦</div>
                    <p class={`${emptyNote} editor-empty`}>No model index yet.</p>
                    {!indexMeta && (
                      <button class="mt-2 text-sm font-semibold text-amber" onClick={() => void refreshIndex()}>Download the model index</button>
                    )}
                  </div>
                </div>
              )}

              {modelsStale.value && results.length === 0 && (
                <div class="empty absolute inset-0 text-center">
                  <p class="text-xs text-amber">Index is stale ({Math.floor((Date.now() - new Date(indexMeta!.generated_at).getTime()) / 86400000)} days old). Showing the cached index.</p>
                </div>
              )}

              {refreshing && results.length === 0 && (
                <div class="empty absolute inset-0 text-center">
                  <p class="text-sm">Refreshing index…</p>
                </div>
              )}

              {modelsRefreshError.value && results.length === 0 && (
                <div class="empty absolute inset-0 text-center">
                  <p class="text-xs text-danger">{modelsRefreshError.value}</p>
                </div>
              )}

              {detailId && detail && view === "list" && (
                <div class="detail-layout relative overflow-hidden">
                  <div
                    ref={listScroller}
                    class="list-panel overflow-y-auto border-r border-line"
                    style={{ height: resultsHeight }}
                  >
                    <div class="absolute inset-0" style={{ height: totalListHeight }}>
                      {visible.map((r, i) => (
                        <Row
                          key={r.id}
                          record={r}
                          index={i}
                          highlightQuery={query}
                          isFocused={focusId === r.id}
                          onFocus={() => setFocusId(r.id)}
                          onSelect={() => {
                            setFocusId(r.id);
                            modelsDetailId.value = r.id;
                            void requestModel(r.id);
                          }}
                          local={localState(r.id)}
                        />
                      ))}
                    </div>
                  </div>
                  <div class="detail-panel overflow-y-auto border-l border-line" style={{ height: resultsHeight, width: 520 }}>
                    {detail ? <ModelDetail record={detail} mode={mode} onDownload={() => void downloadModel(detail.id)} onUse={() => void useModel(detail.id)} /> : (
                      <div class="p-4 text-center text-muted">Select a model to inspect.</div>
                    )}
                  </div>
                </div>
              )}

              {detailId && detail && view === "grid" && (
                <div class="sheet absolute inset-0 z-10 bg-[#17181d]">
                  <div class="flex items-center justify-between px-4 py-2 border-b border-line">
                    <button class="nav text-text" onClick={() => setFocusId(null)}>‹ Back</button>
                    <h2 class="text-sm font-semibold">{detail.metadata.name}</h2>
                    <button class="text-muted" onClick={() => setFocusId(null)} aria-label="Close detail">✕</button>
                  </div>
                  <div class="h-full overflow-y-auto">
                    <ModelDetail record={detail} mode={mode} onDownload={() => void downloadModel(detail.id)} onUse={() => void useModel(detail.id)} />
                  </div>
                </div>
              )}

              {(!detailId || view !== "list") && results.map((r, i) => (
                <div key={r.id} class="results-row" onClick={() => { setFocusId(r.id); modelsDetailId.value = r.id; void requestModel(r.id); }}>
                  {view === "grid" ? <ModelCard record={r} isFocused={focusId === r.id} /> : <Row record={r} index={i} highlightQuery={query} isFocused={focusId === r.id} onFocus={() => setFocusId(r.id)} onSelect={() => { setFocusId(r.id); modelsDetailId.value = r.id; void requestModel(r.id); }} local={localState(r.id)} />}
                </div>
              ))}
            </div>

            {mode === "picker" && !detailId && (
              <div class="hint flex items-center justify-between px-3.5 py-2 border-t border-line-soft flex-shrink-0 text-[10px] text-muted">
                <span>Select a model, then "Use in NAM block".</span>
                <button class="text-amber font-semibold" onClick={() => void openModelsPicker(pickerChannel!)}>Cancel</button>
              </div>
            )}
            {mode === "browse" && (
              <div class="hint flex items-center justify-between px-3.5 py-2 border-t border-line-soft flex-shrink-0 text-[10px] text-muted">
                <span>Downloads are per-model and never destructive.</span>
              </div>
            )}
          </div>
        </div>
      )}
    </>
  );
}

// --- Supporting components + helpers ---------------------------------------

type FacetField = "kinds" | "manufacturers" | "cabIr" | "tags" | "formats" | "state";

interface FacetGroup {
  label: string;
  field: FacetField;
  values: Array<{ value: string; count: number }>;
}

function facetGroups(): FacetGroup[] {
  const f = modelsFacets.value;
  return [
    { label: "Kind", field: "kinds", values: (f.kind as Array<{ value: string; count: number }>) ?? [] },
    { label: "Manufacturer", field: "manufacturers", values: (f.manufacturer as Array<{ value: string; count: number }>) ?? [] },
    { label: "Cab / IR", field: "cabIr", values: (f.cabIr as Array<{ value: string; count: number }>) ?? [] },
    { label: "Tags", field: "tags", values: (f.tags as Array<{ value: string; count: number }>) ?? [] },
    { label: "Format", field: "formats", values: (f.format as Array<{ value: string; count: number }>) ?? [] },
    { label: "State", field: "state", values: (f.state as Array<{ value: string; count: number }>) ?? [] },
  ];
}

function toggleFacet(field: FacetField, value: string) {
  const selected = modelsSelectedFacets.value;
  if (field === "state") {
    const next = selected.states[0] === "all"
    ? (["downloaded", "not-downloaded", "stale"] as ModelState[])
    : (["all"] as ModelState[]);
    modelsSelectedFacets.value = { ...selected, states: next };
    return;
  }
  const current = (selected[field] as string[]);
  const next = current.includes(value) ? current.filter((v) => v !== value) : [...current, value];
  modelsSelectedFacets.value = { ...selected, [field]: next };
}

function toggleFacetState() {
  const selected = modelsSelectedFacets.value;
  const next = selected.states[0] === "all"
    ? (["downloaded", "not-downloaded", "stale"] as ModelState[])
    : (["all"] as ModelState[]);
  modelsSelectedFacets.value = { ...selected, states: next };
}

function clearFilters() {
  modelsSelectedFacets.value = { kinds: [], manufacturers: [], cabIr: [], tags: [], formats: [], states: ["all"] };
}

function Chip({ label, onOff }: { label: string; onOff: () => void }) {
  return (
    <button class="chip px-2.5 py-1 rounded-full border text-[10px] font-semibold bg-amber-bg border-amber-line text-amber-text hover:bg-amber" onClick={onOff}>
      {label} ✕
    </button>
  );
}

interface RowProps {
  record: ModelRecord;
  index: number;
  highlightQuery: string;
  isFocused: boolean;
  onFocus: () => void;
  onSelect: () => void;
  local: LocalState | null;
}

function Row({ record, isFocused, onFocus, onSelect, local, highlightQuery }: RowProps) {
  const downloaded = isDownloaded(record);
  const stale = isStale(record);
  return (
    <button
      class={`row w-full text-left border-b border-line-soft px-3 py-2 flex items-center gap-3 hover:bg-panel-2 transition-colors focus:bg-panel-2 focus:outline-none focus:ring-2 focus:ring-amber ${isFocused ? "bg-panel-2" : ""}`}
      onFocus={onFocus}
      onClick={onSelect}
      title="Enter to select, d to download, u to use (picker)"
    >
      <Thumbnail record={record} size="sm" />
      <div class="min-w-0 flex-1">
        <div class="text-sm font-medium truncate">
          <span dangerouslySetInnerHTML={{ __html: highlight(record.metadata.name, highlightQuery) }} />
        </div>
        <div class="text-[10px] text-muted truncate mt-0.5">{authorText(record)}</div>
        <div class="text-[10px] text-muted mt-0.5">
          {topTags(record).map((t) => <span class="inline-block mr-1 rounded bg-inset px-1">#{t}</span>)}
        </div>
      </div>
      <div class="shrink-0 text-right text-[10px] text-muted min-w-[80px] text-right">
        {downloaded ? (
          <span class={`font-semibold ${stale ? "text-amber" : "text-ok"}`}>{stale ? "update" : "download"}</span>
        ) : (
          <span class="text-muted">—</span>
        )}
        {local && <span class="block text-[9px] font-mono truncate">{local.file_name}</span>}
      </div>
    </button>
  );
}

function ModelCard({ record, isFocused }: { record: ModelRecord; isFocused: boolean }) {
  const downloaded = isDownloaded(record);
  const stale = isStale(record);
  return (
    <button
      class={`card grid gap-2 p-2 rounded-lg border text-left transition-colors focus:bg-panel-2 focus:outline-none focus:ring-2 focus:ring-amber ${isFocused ? "bg-panel-2" : "bg-inset hover:bg-panel-2 border-transparent hover:border-line"}`}
      onClick={() => {
        modelsDetailId.value = record.id;
        void requestModel(record.id);
      }}
      title="Enter to select, d to download"
    >
      <Thumbnail record={record} size="sm" />
      <div class="min-w-0">
        <div class="text-sm font-medium truncate">{record.metadata.name}</div>
        <div class="text-[10px] text-muted truncate">{authorName(record)}</div>
        <div class="text-[10px] text-muted">
          {topTags(record).map((t) => <span class="inline-block mr-1 rounded bg-inset px-1">#{t}</span>)}
        </div>
        <div class="text-[10px] text-muted mt-1">
          {downloaded ? (stale ? <span class="text-amber">update ✓</span> : <span class="text-ok">✓ downloaded</span>) : <span class="text-muted">not downloaded</span>}
        </div>
      </div>
    </button>
  );
}
