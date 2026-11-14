#!/bin/sh
# Fill in the iPlug2 submodule and the VST3 SDK it needs.
#
#   plugin/setup.sh
#
# The VST3 SDK is not committed (it is Steinberg's, fetched from
# steinbergmedia/vst3sdk); the script iPlug2 ships clones it into
# Dependencies/IPlug/VST3_SDK.
set -e

HERE=$(cd "$(dirname "$0")" && pwd)
cd "$HERE"

git submodule update --init --depth 1

if [ ! -f third_party/iPlug2/Dependencies/IPlug/VST3_SDK/CMakeLists.txt ]; then
  ./third_party/iPlug2/Dependencies/IPlug/download-vst3-sdk.sh master
fi
