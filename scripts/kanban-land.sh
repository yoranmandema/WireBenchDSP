#!/usr/bin/env bash
# Land a card's work on main: the only way kanban work reaches the user's checkout.
#
#     scripts/kanban-land.sh <card id>      # run from inside .worktrees/<card id>
#
# Run it at the very end, right before kanban_complete:
# - the reviewer, when approving a builder card;
# - the architect, for its own design notes;
# - anyone who committed fixes on a feature card.
#
# Steps: check the worktree is clean, merge the latest main into the card
# branch, run the checks for what the card changed, then fast-forward the
# user's main checkout to it. Fast-forward only: if anything is off, it stops,
# prints why, and changes nothing on main.
set -euo pipefail

REPO=/Users/yoran/Documents/Repos/pedal-dsp
die() { echo "kanban-land: $*" >&2; exit 1; }

id="${1:?usage: scripts/kanban-land.sh <card id>}"
br="wt/$id"
wt="$(git rev-parse --show-toplevel)"
cd "$wt"

[ "$(git branch --show-current)" = "$br" ] || die "run this inside the card's worktree, on branch $br (you are on '$(git branch --show-current)' in $wt)"
[ -z "$(git status --porcelain)" ] || die "the worktree has uncommitted changes. Commit them (or stash what doesn't belong), then rerun"
[ "$(git -C "$REPO" branch --show-current)" = "main" ] || die "the user's checkout ($REPO) is not on main. Block with needs_input; don't switch it yourself"

if ! git merge-base --is-ancestor main HEAD; then
  git merge --no-edit -q main || die "merging main into $br conflicts. Resolve it here, commit, rerun"
fi

changed="$(git diff --name-only main...HEAD)"
if [ -z "$changed" ]; then
  echo "kanban-land: $br has nothing that isn't already on main. Nothing to land"
  exit 0
fi

# shellcheck source=/dev/null
source "$wt/scripts/agent-env.sh" >/dev/null
if grep -q -E '(\.rs|Cargo\.(toml|lock))$' <<<"$changed"; then
  echo "kanban-land: Rust changed: cargo test"
  cargo test --workspace --exclude pedal-shell -q >"$TMPDIR/kanban-land-$id.log" 2>&1 \
    || { tail -30 "$TMPDIR/kanban-land-$id.log"; die "cargo test failed (full log: $TMPDIR/kanban-land-$id.log)"; }
fi
if grep -q '^ui/' <<<"$changed"; then
  echo "kanban-land: UI changed: typecheck + build"
  ( cd ui && pnpm run build ) >/tmp/kanban-land-$id.log 2>&1 \
    || { tail -30 /tmp/kanban-land-$id.log; die "UI build failed (full log: /tmp/kanban-land-$id.log)"; }
fi

git -C "$REPO" merge --ff-only -q "$br" \
  || die "could not fast-forward main. The user probably has local edits in the same files. Block with needs_input"
echo "kanban-land: landed $br on main ($(git rev-parse --short HEAD)):"
git --no-pager log --oneline "main@{1}..main" 2>/dev/null | sed 's/^/  /' || true
