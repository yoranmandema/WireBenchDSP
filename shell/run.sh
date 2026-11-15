#!/usr/bin/env bash
# Launch Pedal with an up-to-date UI.
#
#   ./shell/run.sh dev     live-reload loop: Vite + the debug shell.
#                          UI edits appear on save; no rebuild needed.
#   ./shell/run.sh ship    the shipped app: rebuild Pedal.app, then open it.
#
# Both modes start the engine themselves. A stale engine left holding port
# 8737 is the usual reason the UI sits on "Connecting…" forever, so it is
# cleared here before anything starts.
#
# How fresh is the UI?
#   dev   -> always current. The debug shell loads Vite's devUrl, not the
#            embedded bundle, so `ui/src` edits show up immediately.
#   ship  -> a snapshot. Pedal.app embeds ui/dist at build time, so it only
#            changes when this script rebuilds it.
set -euo pipefail
cd "$(dirname "$0")/.."

mode="${1:-dev}"
PORT=8737

port_busy() { lsof -iTCP:"$PORT" -sTCP:LISTEN -n >/dev/null 2>&1; }

# The shell's devUrl is http://localhost:5173 (tauri.conf.json). If another Vite
# is already there, ours silently moves to 5174 and the shell still loads 5173,
# so it would show someone else's session. Clear the port and pin ours to it.
free_vite() {
  { lsof -iTCP:5173 -sTCP:LISTEN -n 2>/dev/null || true; } | tail -n +2 | awk '{print $2}' | sort -u |
    while read -r pid; do
      [ -n "$pid" ] || continue
      echo "    stopping stale Vite pid $pid"
      kill -9 "$pid" 2>/dev/null || true
    done
  sleep 1
}

free_port() {
  if port_busy; then
    echo "• port $PORT is held — stopping the stale engine:"
    { lsof -iTCP:"$PORT" -sTCP:LISTEN -n || true; } | tail -n +2 | awk '{print $2}' | sort -u |
      while read -r pid; do
        [ -n "$pid" ] || continue
        echo "    killing pid $pid"
        kill -9 "$pid" 2>/dev/null || true
      done
    sleep 1
  fi
  if port_busy; then
    echo "✗ port $PORT is still busy — close whatever is using it and retry."
    exit 1
  fi
}

case "$mode" in
  dev)
    free_port
    free_vite
    # NOTE: no --features custom-protocol. A plain debug build is a Tauri *dev*
    # build and loads Vite's devUrl; that is exactly what gives live reload.
    cargo build -p pedal-shell
    echo "• starting Vite on http://localhost:5173"
    ( cd ui && pnpm dev -- --port 5173 --strictPort ) &
    vite=$!
    trap 'kill $vite 2>/dev/null || true' EXIT
    echo "• starting the shell (it spawns the engine)"
    ./target/debug/pedal-shell
    ;;

  ship)
    free_port
    ./shell/build-bundle.sh
    open target/release/bundle/macos/Pedal.app
    ;;

  *)
    echo "usage: $0 [dev|ship]" >&2
    exit 2
    ;;
esac
