#!/bin/sh
# Build the pedal_ffi staticlib and run the C harness against it.
#
#   ffi/test-c.sh                          # presets/04-rainbow.json
#   ffi/test-c.sh presets/00-blank.json
#   PEDAL_FFI_PROFILE=release ffi/test-c.sh
#
# The harness writes its audio to a buffer and never opens a device, so this
# is safe to run on a machine with attached speakers.
set -e

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

PROFILE=${PEDAL_FFI_PROFILE:-dev}
cargo build -p pedal-ffi --profile "$PROFILE"
# Cargo maps the `dev` profile onto `target/debug`; every other profile uses
# its own directory.
LIBDIR=$PROFILE
[ "$PROFILE" = "dev" ] && LIBDIR=debug
LIB="target/$LIBDIR/libpedal_ffi.a"

OUT=${TMPDIR:-/tmp}/pedal_ffi_c_harness
clang -O2 -Wall -Wextra -I ffi/include ffi/tests/c_harness.c "$LIB" \
    -o "$OUT" -lc -lm

"$OUT" "$@"
