#!/bin/sh
# Build the plugin and load it in a minimal headless VST3 host.
#
#   plugin/test-vst3.sh [seconds]
#
# The host instantiates the bundle, runs a 220 Hz sine through the DSP at
# 48 kHz and writes the output to a raw file. It never opens an audio device
# and never plays anything, so it is safe on a machine with speakers.
set -e

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/.." && pwd)
cd "$ROOT"

DURATION=${1:-1}
SDK="$HERE/third_party/iPlug2/Dependencies/IPlug/VST3_SDK"
BUNDLE="$HERE/build/out/PedalVST3.vst3"
HOST_BIN="$HERE/build/host/vst3_host"

if [ ! -f "$SDK/public.sdk/source/vst/hosting/hostclasses.h" ]; then
  echo "VST3 SDK missing — run plugin/setup.sh" >&2
  exit 1
fi

# 1. the Rust core + the .vst3 bundle
cargo build -p pedal-ffi --release
cmake --build "$HERE/build" --config Release

# 2. the host. Compiled straight against the SDK sources, because the SDK's
#    own CMake requires a full Xcode install and this host only needs the
#    base + hosting layers.
mkdir -p "$HERE/build/host"

SRCS="$HERE/tests/vst3_host.mm"
for f in \
  base/source/baseiids.cpp base/source/fdebug.cpp base/source/fobject.cpp \
  base/source/fstring.cpp base/source/timer.cpp base/source/updatehandler.cpp \
  base/thread/source/flock.cpp \
  pluginterfaces/base/coreiids.cpp pluginterfaces/base/funknown.cpp \
  pluginterfaces/base/ustring.cpp \
  public.sdk/source/common/commoniids.cpp \
  public.sdk/source/common/commonstringconvert.cpp \
  public.sdk/source/common/pluginview.cpp \
  public.sdk/source/main/pluginfactory.cpp \
  public.sdk/source/vst/vstbus.cpp public.sdk/source/vst/vstcomponent.cpp \
  public.sdk/source/vst/vstcomponentbase.cpp public.sdk/source/vst/vstinitiids.cpp \
  public.sdk/source/vst/vstparameters.cpp \
  public.sdk/source/vst/vstsinglecomponenteffect.cpp \
  public.sdk/source/vst/hosting/hostclasses.cpp \
  public.sdk/source/vst/hosting/parameterchanges.cpp \
  public.sdk/source/vst/hosting/processdata.cpp \
  public.sdk/source/vst/hosting/eventlist.cpp \
  public.sdk/source/vst/hosting/pluginterfacesupport.cpp \
  public.sdk/source/vst/utility/stringconvert.cpp \
  public.sdk/source/vst/utility/systemtime.cpp \
  public.sdk/source/vst/utility/dataexchange.cpp \
  public.sdk/source/vst/utility/mpeprocessor.cpp \
  public.sdk/source/vst/utility/vst2persistence.cpp
do
  SRCS="$SRCS $SDK/$f"
done

clang++ -std=c++17 -O2 -g -arch arm64 -mmacosx-version-min=13.0 \
  -DRELEASE -DNDEBUG -DSMTG_RENAME_ASSERT=0 \
  -I"$SDK" -I"$SDK/pluginterfaces" -I"$SDK/pluginterfaces/base" \
  -I"$SDK/pluginterfaces/vst" -I"$SDK/public.sdk" -I"$SDK/public.sdk/source" \
  -I"$SDK/public.sdk/source/vst" -I"$SDK/public.sdk/source/vst/hosting" \
  -I"$SDK/public.sdk/source/vst/utility" -I"$SDK/base" \
  -I"$SDK/base/source" -I"$SDK/base/thread/source" \
  $SRCS -o "$HOST_BIN" \
  -framework Cocoa -framework Foundation -framework CoreAudio -liconv

# 3. run it
rm -f /tmp/pedal-vst3-out.raw
"$HOST_BIN" "$BUNDLE" /tmp/pedal-vst3-out.raw "$DURATION"

echo
echo "raw output: /tmp/pedal-vst3-out.raw"
