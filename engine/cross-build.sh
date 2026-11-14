#!/usr/bin/env bash
# Cross-build the engine for the Pi 5 (`aarch64-unknown-linux-gnu`).
#
# The engine links ALSA through cpal, and Rust's `pkg-config` crate refuses to
# cross-compile without a target sysroot. So this script assembles three things:
#
#   1. an aarch64 Linux toolchain (messense/homebrew-macos-cross-toolchains),
#   2. libasound for aarch64, built against that toolchain's sysroot,
#   3. the engine, with the linker and pkg-config pointed at both.
#
# The result is a genuine aarch64 ELF. Raspberry Pi OS ships `libasound.so.2`
# (package `libasound2t64`), which is the only library the binary needs beyond
# libc — see `docs/PI-DEPLOY.md`.
#
# Everything is cached under $PEDAL_CROSS_HOME (default ~/.cache/pedal-cross),
# so a second run only rebuilds the engine.
#
# Usage: engine/cross-build.sh [--cargo flags]
#   engine/cross-build.sh            # release build
#   engine/cross-build.sh -- -p pedal-engine --profile dev

set -euo pipefail

TARGET=aarch64-unknown-linux-gnu
TC_VERSION=15.2.0
ALSA_VERSION=1.2.14

ROOT="${PEDAL_CROSS_HOME:-$HOME/.cache/pedal-cross}"
HOST_ARCH="$(uname -m)"          # arm64 | x86_64
TC_URL="https://github.com/messense/homebrew-macos-cross-toolchains/releases/download/v${TC_VERSION}/${TARGET}-${HOST_ARCH}-darwin.tar.gz"
ALSA_URL="https://www.alsa-project.org/files/pub/lib/alsa-lib-${ALSA_VERSION}.tar.bz2"

# The toolchain's own triplet is aarch64-linux-gnu; Rust's target adds `unknown`.
CROSS_GCC_NAME=aarch64-linux-gnu-gcc

mkdir -p "$ROOT"
TC="$ROOT/toolchain"
ALSA="$ROOT/alsa-sysroot"

# --- 1. toolchain ---------------------------------------------------------
if [ ! -x "$TC/bin/$CROSS_GCC_NAME" ]; then
  echo "==> toolchain $TC_VERSION ($HOST_ARCH host)"
  curl -sL "$TC_URL" -o "$ROOT/toolchain.tar.gz"
  rm -rf "$TC"
  mkdir -p "$TC"
  tar xzf "$ROOT/toolchain.tar.gz" -C "$TC" --strip-components=1
  rm "$ROOT/toolchain.tar.gz"
fi

# --- 2. libasound for aarch64 ---------------------------------------------
if [ ! -f "$ALSA/lib/libasound.so" ]; then
  echo "==> libasound $ALSA_VERSION for aarch64"
  curl -sL "$ALSA_URL" -o "$ROOT/alsa-lib.tar.bz2"
  rm -rf "$ROOT/alsa-lib-$ALSA_VERSION"
  tar xjf "$ROOT/alsa-lib.tar.bz2" -C "$ROOT"
  cd "$ROOT/alsa-lib-$ALSA_VERSION"
  PATH="$TC/bin:$PATH" \
    ./configure --host=aarch64-linux-gnu --prefix="$ALSA" --disable-python
  make -j"$(sysctl -n hw.ncpu 2>/dev/null || nproc)"
  make install
fi

# --- 3. the engine --------------------------------------------------------
if ! rustup target list --installed | grep -qx "$TARGET"; then
  echo "==> rustup target add $TARGET"
  rustup target add "$TARGET"
fi

cd "$(dirname "$0")/.."
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$TC/bin/$CROSS_GCC_NAME"
export PKG_CONFIG_PATH="$ALSA/lib/pkgconfig"
# The cross gcc carries its own sysroot for libc, so pkg-config only has to
# find alsa; the crate's "no cross pkg-config" guard is bypassed.
export PKG_CONFIG_ALLOW_CROSS=1

echo "==> cargo build -p pedal-engine --release --target $TARGET"
cargo build -p pedal-engine --release --target "$TARGET" "$@"

BIN="target/$TARGET/release/pedal-engine"
echo
echo "built $BIN"
file "$BIN"
"$TC/bin/aarch64-linux-gnu-readelf" -d "$BIN" | grep NEEDED || true
echo
echo "next: docs/PI-DEPLOY.md"
