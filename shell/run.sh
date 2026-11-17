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
    # Node + pnpm: prefer your own install; fall back to the toolchain Hermes
    # ships in ~/.hermes/tools (which kanban workers use) so this works either way.
    if ! command -v node >/dev/null 2>&1; then
      for d in "$HOME"/.hermes/tools/node-*/bin; do [ -x "$d/node" ] && PATH="$d:$PATH"; done
    fi
    if ! command -v pnpm >/dev/null 2>&1; then
      for d in "$HOME"/.hermes/tools/pnpm-*; do [ -x "$d/pnpm" ] && PATH="$d:$PATH"; done
    fi
    export PATH
    if ! command -v node >/dev/null 2>&1 || ! command -v pnpm >/dev/null 2>&1; then
      echo "✗ node/pnpm not found. Install them with:  brew install node pnpm" >&2
      exit 1
    fi
    [ -d ui/node_modules ] || ( cd ui && pnpm install --frozen-lockfile )
    echo "• starting Vite on http://localhost:5173"
    ( cd ui && pnpm dev --port 5173 --strictPort ) &
    vite=$!
    trap 'kill $vite 2>/dev/null || true' EXIT
    # Don't open the window until Vite answers, or the webview shows a blank page.
    for _ in $(seq 60); do
      lsof -iTCP:5173 -sTCP:LISTEN -n >/dev/null 2>&1 && break
      kill -0 "$vite" 2>/dev/null || { echo "✗ Vite exited — see the error above" >&2; exit 1; }
      sleep 0.5
    done
    lsof -iTCP:5173 -sTCP:LISTEN -n >/dev/null 2>&1 || { echo "✗ Vite did not start within 30 s" >&2; exit 1; }
    echo "• starting the shell (it spawns the engine)"
    ./target/debug/pedal-shell &
    shell=$!
    # The shell spawns the engine, which takes a moment to bind :8737. Until it
    # does, Vite's /engine proxy logs ECONNREFUSED and the UI sits on the boot
    # screen -- which reads as "the app is broken". Wait for the port.
    echo -n "• waiting for the engine on :$PORT"
    for _ in $(seq 1 60); do
      if port_busy; then echo " up"; break; fi
      if ! kill -0 "$shell" 2>/dev/null; then
        echo
        echo "✗ the shell exited before the engine came up. Engine log:"
        tail -20 /tmp/pedal-engine.log 2>/dev/null
        exit 1
      fi
      echo -n "."
      sleep 0.5
    done
    if ! port_busy; then
      echo
      echo "✗ the engine never listened on :$PORT. Engine log:"
      tail -20 /tmp/pedal-engine.log 2>/dev/null
      exit 1
    fi
    wait "$shell"
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
