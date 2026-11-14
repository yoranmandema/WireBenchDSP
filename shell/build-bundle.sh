#!/usr/bin/env bash
#
# Build the self-contained macOS app *and* its .dmg installer.
#
# Why CI=true:
# tauri-bundler's DMG step drives Finder through AppleScript to prettify the
# volume (background picture, icon positions). That AppleEvent times out
# (-1712) or is denied (-1743) in any session without a Finder GUI session --
# cron, SSH, CI. Setting CI=true makes the bundler pass `--skip-jenkins`, which
# skips only the cosmetic pass; the DMG still carries Pedal.app and the
# Applications symlink. See tauri-apps/tauri#592.
set -euo pipefail
cd "$(dirname "$0")"

# A failed DMG run leaves its read-write image behind in bundle/macos.
rm -f ../target/release/bundle/macos/rw.*.dmg

CI=true npx --yes @tauri-apps/cli build
