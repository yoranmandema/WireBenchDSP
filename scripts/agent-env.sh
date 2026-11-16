# Source this once at the start of every kanban session, from inside your
# card's worktree:
#
#     source scripts/agent-env.sh
#
# - Puts node/pnpm on PATH (Hermes ships them in ~/.hermes/tools).
# - Points cargo at one build directory shared by all worktrees, so each new
#   worktree doesn't rebuild the whole workspace from scratch. It's separate
#   from the main checkout's target/, which the user's app runs from.
# - Installs ui/node_modules in this worktree if it's missing (from the local
#   pnpm store, so it takes seconds).
# - Brings this branch up to date with main.

if ! command -v node >/dev/null 2>&1; then
  for d in "$HOME"/.hermes/tools/node-*/bin; do [ -x "$d/node" ] && PATH="$d:$PATH"; done
fi
if ! command -v pnpm >/dev/null 2>&1; then
  for d in "$HOME"/.hermes/tools/pnpm-*; do [ -x "$d/pnpm" ] && PATH="$d:$PATH"; done
fi
export PATH
export CARGO_TARGET_DIR="$HOME/.cache/pedal-dsp/target"
export PEDAL_ENGINE_BIN="$CARGO_TARGET_DIR/debug/pedal-engine"

_pedal_top="$(git rev-parse --show-toplevel 2>/dev/null)"
if [ -n "$_pedal_top" ]; then
  if [ ! -d "$_pedal_top/ui/node_modules" ]; then
    ( cd "$_pedal_top/ui" && { pnpm install --frozen-lockfile --prefer-offline >/dev/null 2>&1 \
        || pnpm install --prefer-offline >/dev/null 2>&1; } ) \
      && echo "agent-env: installed ui/node_modules" \
      || echo "agent-env: pnpm install FAILED in ui/ — run it yourself to see why"
  fi
  case "$(git -C "$_pedal_top" branch --show-current)" in
    wt/*)
      if [ -z "$(git -C "$_pedal_top" status --porcelain)" ] && ! git -C "$_pedal_top" merge-base --is-ancestor main HEAD; then
        git -C "$_pedal_top" merge --no-edit -q main \
          && echo "agent-env: merged the latest main into $(git -C "$_pedal_top" branch --show-current)" \
          || echo "agent-env: merging main hit a conflict — resolve it and commit before anything else"
      fi ;;
    main) echo "agent-env: WARNING — you are in the main checkout, not a card worktree. Kanban work belongs in .worktrees/<card id>." ;;
  esac
fi
unset _pedal_top
echo "agent-env: CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
