import { state, addBlock } from "../state";
import { BlockList, groupBlocks, type BlockEntry } from "./BlockList";

/** The left rack: every block type the engine offers, grouped by category. */
export function Rack() {
  const s = state.value;
  if (!s) return null;

  const pick = ({ type, name }: BlockEntry) => {
    const ch = s.channels[0];
    if (ch) void addBlock(ch.id, type, name);
  };

  return (
    <aside class="rack bg-panel border-r border-line overflow-y-auto p-2.5">
      <BlockList groups={groupBlocks(s.registry)} onSelect={pick} />
    </aside>
  );
}