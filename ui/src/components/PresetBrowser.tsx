import { useEffect, useRef, useState } from "preact/hooks";
import {
  deletePreset,
  loadPreset,
  presetsOpen,
  renamePreset,
  savePreset,
  state,
} from "../state";

/** Mirror of the engine's file-name slugging, for duplicate checks. */
function slugify(name: string): string {
  return name.replace(/[^a-zA-Z0-9_-]/g, "-");
}

/**
 * The preset browser (US-2.9): list every preset, load one, save the current
 * rig under a new name, rename, and delete. Every action goes through the
 * bridge; the engine answers with a fresh snapshot.
 */
export function PresetBrowser() {
  const s = state.value;
  const [newName, setNewName] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameTo, setRenameTo] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const renameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (renaming) renameRef.current?.focus();
  }, [renaming]);

  if (!presetsOpen.value || !s) return null;

  const close = () => {
    presetsOpen.value = false;
    setErr(null);
    setRenaming(null);
  };

  const run = async (p: Promise<unknown>) => {
    setBusy(true);
    setErr(null);
    try {
      await p;
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
    setBusy(false);
  };

  const save = async () => {
    const slug = slugify(newName.trim());
    if (!slug) return;
    await run(savePreset(slug));
    setNewName("");
  };

  const rename = async (from: string) => {
    const slug = slugify(renameTo.trim());
    if (!slug || slug === from) {
      setRenaming(null);
      return;
    }
    await run(renamePreset(from, slug));
    setRenaming(null);
  };

  const remove = async (name: string) => {
    if (!window.confirm(`Delete preset "${name}"?`)) return;
    await run(deletePreset(name));
  };

  const slug = slugify(newName.trim());
  const duplicate = slug !== "" && s.presets.includes(slug);

  return (
    <div class="modal-backdrop" onClick={close}>
      <div class="modal" onClick={(e) => e.stopPropagation()}>
        <header class="modal-head">
          <h2>Presets</h2>
          <button class="nav" onClick={close} title="Close">
            ×
          </button>
        </header>

        {err && <div class="modal-error">{err}</div>}

        <div class="preset-list">
          {s.presets.length === 0 && (
            <div class="editor-empty">No presets yet — save the current rig below.</div>
          )}
          {s.presets.map((name) => (
            <div key={name} class={`preset-row${name === s.preset ? " current" : ""}`}>
              {renaming === name ? (
                <input
                  class="preset-rename"
                  ref={renameRef}
                  value={renameTo}
                  title="New preset name"
                  onInput={(e) => setRenameTo((e.target as HTMLInputElement).value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void rename(name);
                    if (e.key === "Escape") setRenaming(null);
                  }}
                  onBlur={() => setRenaming(null)}
                />
              ) : (
                <button
                  class="preset-load"
                  onClick={() => void run(loadPreset(name))}
                  title={`Load ${name}`}
                >
                  {name}
                  {name === s.preset && <span class="preset-current">current</span>}
                </button>
              )}
              <button
                class="mini"
                onClick={() => {
                  setRenaming(name);
                  setRenameTo(name);
                }}
                title="Rename preset"
              >
                Rename
              </button>
              <button class="mini danger" onClick={() => void remove(name)} title="Delete preset">
                Delete
              </button>
            </div>
          ))}
        </div>

        <footer class="modal-foot">
          <input
            class="preset-new"
            placeholder="New preset name"
            value={newName}
            title="Name for the current rig"
            onInput={(e) => setNewName((e.target as HTMLInputElement).value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void save();
            }}
          />
          <button class="btn" disabled={!slug || duplicate || busy} onClick={() => void save()}>
            Save
          </button>
          {duplicate && <span class="modal-hint">That name already exists.</span>}
        </footer>
      </div>
    </div>
  );
}
