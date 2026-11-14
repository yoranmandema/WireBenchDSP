#!/bin/sh
# Build the PedalVST3 bundle.
#
#   plugin/build.sh          # build into plugin/build
#   plugin/build.sh deploy   # also symlink into ~/Library/Audio/Plug-Ins/VST3
#
# Produces plugin/build/out/PedalVST3.vst3. Safe to run on a machine with
# attached speakers: nothing here opens an audio device.
set -e

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/.." && pwd)
cd "$ROOT"

cargo build -p pedal-ffi --release

cd "$HERE"
cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release

echo
echo "Built: $HERE/build/out/PedalVST3.vst3"

if [ "$1" = "deploy" ]; then
  DEST="$HOME/Library/Audio/Plug-Ins/VST3"
  mkdir -p "$DEST"
  rm -rf "$DEST/PedalVST3.vst3"
  ln -sf "$HERE/build/out/PedalVST3.vst3" "$DEST/PedalVST3.vst3"
  echo "Deployed: $DEST/PedalVST3.vst3"
fi
